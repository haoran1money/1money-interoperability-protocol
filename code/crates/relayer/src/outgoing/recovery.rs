use core::sync::atomic::Ordering;

use alloy_primitives::FixedBytes;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types_eth::{BlockNumberOrTag, Filter};
use alloy_sol_types::SolEvent;
use onemoney_interop::contract::OMInterop::{self, OMInteropSent};
use onemoney_interop::contract::TxHashMapping;
use onemoney_protocol::{CheckpointTransactions, Client, TxPayload};
use tracing::warn;

use crate::config::{Config, RelayerNonce};
use crate::outgoing::error::Error;

const MAX_BLOCK_RANGE: u64 = 100_000;

pub async fn get_earliest_incomplete_checkpoint_number(config: &Config) -> Result<u64, Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let contract = OMInterop::new(config.interop_contract_address, provider);

    let res = contract.getLatestCompletedCheckpoint().call().await?;
    Ok(res)
}

pub async fn recover_incomplete_withdrawals_hash_mapping(
    config: &Config,
    relayer_nonce: RelayerNonce,
    start_checkpoint: Option<u64>,
    start_block: Option<u64>,
) -> Result<(), Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider.clone());

    let incomplete_hashes = mapping_contract.incompleteWithdrawals().call().await?;

    let client = Client::custom(config.one_money_node_url.to_string())?;

    // If a start checkpoint has been given use it, else start from 0
    let start = start_checkpoint.unwrap_or_default();
    // If a start block has been given use it, else start from 0
    let start_block = start_block.unwrap_or_default();

    let last_checkpoint = client.get_checkpoint_number().await?.number;

    'hash_loop: for hash in incomplete_hashes.iter() {
        let tx_hash = *hash;
        let withdrawal_hashes = mapping_contract.getWithdrawal(tx_hash).call().await?;

        let tx_receipt = match client
            .get_transaction_receipt_by_hash(&hash.to_string())
            .await
        {
            Ok(tx_receipt) => tx_receipt,
            Err(e) => {
                warn!("Failed to query `{tx_hash}` receipt, most likely due to the receipt not existing. The mapping will be done when recovering transactions. Cause {e}");
                continue 'hash_loop;
            }
        };

        let latest: u64 = provider.get_block_number().await?;
        let deployment_block: u64 = start_block;
        let mut from_block = deployment_block;

        let mut bridge_to_tx_hash = None;
        let mut refund_amount = None;
        let mut om_token = None;
        let mut sidechain_nonce = None;
        let mut from = None;

        while from_block <= latest {
            let to = (from_block + MAX_BLOCK_RANGE - 1).min(latest);

            // The filter is safe because `from` and `sourceHash` are indexed in `OMInteropSent`
            let filter = Filter::new()
                .address(config.interop_contract_address)
                .topic1(tx_receipt.from)
                .topic3(tx_hash)
                .from_block(BlockNumberOrTag::Number(from_block))
                .to_block(BlockNumberOrTag::Number(to));

            let logs = provider.get_logs(&filter).await?;

            for log in logs {
                if let Ok(parsed) = OMInteropSent::decode_raw_log(log.topics(), &log.data().data) {
                    if parsed.sourceHash != tx_hash || parsed.from != tx_receipt.from {
                        continue;
                    }

                    if let Some(transaction_hash) = log.transaction_hash {
                        bridge_to_tx_hash = Some(transaction_hash);
                        refund_amount = Some(parsed.refundAmount);
                        om_token = Some(parsed.omToken);
                        sidechain_nonce = Some(parsed.nonce);
                        from = Some(parsed.from);
                        break;
                    }
                }
            }

            if bridge_to_tx_hash.is_some() {
                break;
            }

            from_block = to + 1;
        }
        let bridge_to_tx_hash = bridge_to_tx_hash.ok_or_else(|| {
            Error::Generic(format!(
                "Failed to retrieve `tx_hash` for transaction `{tx_hash}`"
            ))
        })?;
        let refund_amount = refund_amount.ok_or_else(|| {
            Error::Generic(format!(
                "Failed to retrieve `refundAmount` for transaction `{tx_hash}`"
            ))
        })?;
        let sidechain_nonce = sidechain_nonce.ok_or_else(|| {
            Error::Generic(format!(
                "Failed to retrieve `nonce` for transaction `{tx_hash}`"
            ))
        })?;
        let from = from.ok_or_else(|| {
            Error::Generic(format!(
                "Failed to retrieve `from` for transaction `{tx_hash}`"
            ))
        })?;

        if withdrawal_hashes.bridgeTo == FixedBytes::ZERO {
            match mapping_contract
                .linkWithdrawalHashes(tx_hash, bridge_to_tx_hash)
                .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
                .send()
                .await
                .map(Ok)
                .or_else(|e| {
                    e.try_decode_into_interface_error::<TxHashMapping::TxHashMappingErrors>()
                        .map(Err)
                })?
                .map_err(Error::MappingContractReverted)
            {
                Ok(pending_tx) => {
                    if let Err(e) = pending_tx.get_receipt().await {
                        warn!(
                            burn_and_bridge_hash=%hash,
                            bridge_to_hash=%bridge_to_tx_hash,
                            error = %e,
                            "Failed to retrieve `linkWithdrawalHashes` receipt"
                        );
                    }
                }
                Err(e) => {
                    // If send failed, decrement the nonce
                    relayer_nonce.fetch_sub(1, Ordering::SeqCst);
                    warn!(
                            burn_and_bridge_hash=%hash,
                            bridge_to_hash=%bridge_to_tx_hash,
                        error = %e,
                        "Failed to link withdrawal hashes"
                    );
                }
            }
        }

        if withdrawal_hashes.refund == FixedBytes::ZERO {
            for i in start..=last_checkpoint {
                let checkpoint = match client.get_checkpoint_by_number(i, true).await {
                    Ok(ch) => ch,
                    Err(e) => {
                        warn!("Failed to query checkpoint `{i}`. Cause: {e}");
                        continue;
                    }
                };

                let maybe_token_transfer_transaction = match &checkpoint.transactions {
                    CheckpointTransactions::Full(txs) => txs.iter().find(|tx| {
                        if let TxPayload::TokenTransfer {
                            value,
                            recipient,
                            token,
                        } = &tx.data
                        {
                            tx.nonce == sidechain_nonce
                                && *value == refund_amount.to_string()
                                && *token == om_token
                                && *recipient == from
                        } else {
                            false
                        }
                    }),
                    _ => None,
                };

                if let Some(token_transfer_transaction) = maybe_token_transfer_transaction {
                    match mapping_contract
                        .linkRefundHashes(tx_hash, token_transfer_transaction.hash)
                        .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
                        .send()
                        .await
                        .map(Ok)
                        .or_else(|e| {
                            e.try_decode_into_interface_error::<TxHashMapping::TxHashMappingErrors>(
                            )
                            .map(Err)
                        })?
                        .map_err(Error::MappingContractReverted)
                    {
                        Ok(pending_tx) => {
                            if let Err(e) = pending_tx.get_receipt().await {
                                warn!(
                                    bridge_from_hash=%hash,
                                    bridge_and_mint_hash=%token_transfer_transaction.hash,
                                    error = %e,
                                    "Failed to retrieve `linkRefundHashes` receipt"
                                );
                            }
                        }
                        Err(e) => {
                            // If send failed, decrement the nonce
                            relayer_nonce.fetch_sub(1, Ordering::SeqCst);
                            warn!(
                                    bridge_from_hash=%hash,
                                    bridge_and_mint_hash=%token_transfer_transaction.hash,
                                error = %e,
                                "Failed to link refund hash"
                            );
                        }
                    }
                    // Process next incomplete deposit hash
                    continue 'hash_loop;
                }
            }
        }
    }

    Ok(())
}
