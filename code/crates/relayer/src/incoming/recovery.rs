use core::sync::atomic::Ordering;

use alloy_primitives::TxHash;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_sol_types::SolEvent;
use onemoney_interop::contract::OMInterop::{self, OMInteropErrors, OMInteropReceived};
use onemoney_interop::contract::TxHashMapping;
use onemoney_protocol::{CheckpointTransactions, Client, TxPayload};
use tracing::warn;

use crate::config::{Config, RelayerNonce};
use crate::incoming::error::Error;

pub async fn get_latest_incomplete_block_number(config: &Config) -> Result<u64, Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let latest_block_number = provider.get_block_number().await?;

    let contract = OMInterop::new(config.interop_contract_address, provider);
    let client = Client::custom(config.one_money_node_url.to_string())?;

    let om_relayer_nonce = client
        .get_account_nonce(config.relayer_private_key.address())
        .await?
        .nonce;
    let sc_relayer_nonce = sc_inbound_nonce_at(&contract, latest_block_number).await?;

    if om_relayer_nonce > sc_relayer_nonce {
        return Err(Error::Generic(format!("Relayer account nonce is bigger on 1Money side. 1Money {om_relayer_nonce}, Sidechain {sc_relayer_nonce}")));
    }

    // If both nonces are 0 this is a special case and we start relaying from
    // block 0
    if om_relayer_nonce == 0 {
        return Ok(0);
    }

    if om_relayer_nonce == sc_relayer_nonce {
        return Ok(latest_block_number);
    }

    let mut low = 0u64;
    let mut high = latest_block_number;
    while low < high {
        let mid = (low + high).div_ceil(2);
        let nonce_mid = sc_inbound_nonce_at(&contract, mid).await?;

        if nonce_mid <= om_relayer_nonce {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    Ok(low)
}

async fn sc_inbound_nonce_at<P: Provider>(
    contract: &onemoney_interop::contract::OMInterop::OMInteropInstance<P>,
    block: u64,
) -> Result<u64, Error> {
    let res = contract
        .getLatestInboundNonce()
        .call()
        .block(block.into())
        .await;

    match res {
        Ok(n) => Ok(n),
        Err(e) => match e.try_decode_into_interface_error::<OMInteropErrors>() {
            Ok(other) => Err(Error::ContractReverted(other)),
            Err(e) => Err(e.into()),
        },
    }
}

pub async fn recover_incomplete_deposit_hash_mapping(
    config: &Config,
    relayer_nonce: RelayerNonce,
    start_checkpoint: Option<u64>,
) -> Result<(), Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider.clone());

    let incomplete_hashes = mapping_contract.incompleteDeposits().call().await?;

    let client = Client::custom(config.one_money_node_url.to_string())?;

    // If a start checkpoint has been given use it, else start from 0
    let start = start_checkpoint.unwrap_or_default();

    let last_checkpoint = client.get_checkpoint_number().await?.number;

    if start > last_checkpoint {
        warn!(
            start,
            last_checkpoint, "Start checkpoint is greater than last checkpoint; nothing to recover"
        );
        return Ok(());
    }

    'hash_loop: for hash in incomplete_hashes.iter() {
        let tx_hash: TxHash = *hash;

        // Get the transaction receipt from the transaction hash
        let receipt = match provider.get_transaction_receipt(tx_hash).await {
            Ok(tx_receipt) => tx_receipt.ok_or_else(|| {
                Error::Generic(format!("Missing transaction receipt for hash `{tx_hash}`"))
            })?,
            Err(e) => {
                warn!("Failed to query `{tx_hash}` receipt, most likely due to the receipt not existing. The mapping will be done when recovering transactions. Cause {e}");
                continue 'hash_loop;
            }
        };

        // Decode OMInteropReceived to retrieve the nonce and account address
        let mut sidechain_nonce = None;
        let mut to = None;

        for log in receipt.logs() {
            if let Ok(ev) = OMInteropReceived::decode_raw_log(log.topics(), &log.data().data) {
                sidechain_nonce = Some(ev.nonce);
                to = Some(ev.to);
                break;
            }
        }

        let sidechain_nonce = sidechain_nonce.ok_or_else(|| {
            Error::Generic(format!(
                "Failed to retrieve `nonce` for transaction `{tx_hash}`"
            ))
        })?;
        let to = to.ok_or_else(|| {
            Error::Generic(format!(
                "Failed to retrieve `to` for transaction `{tx_hash}`"
            ))
        })?;

        for i in start..=last_checkpoint {
            let checkpoint = match client.get_checkpoint_by_number(i, true).await {
                Ok(ch) => ch,
                Err(e) => {
                    warn!("Failed to query checkpoint `{i}`. Cause: {e}");
                    continue;
                }
            };

            let maybe_bridge_and_mint_transaction = match &checkpoint.transactions {
                CheckpointTransactions::Full(txs) => txs.iter().find(|tx| {
                    if let TxPayload::TokenBridgeAndMint { recipient, .. } = &tx.data {
                        tx.nonce == sidechain_nonce && *recipient == to
                    } else {
                        false
                    }
                }),
                _ => None,
            };

            if let Some(bridge_and_mint_transaction) = maybe_bridge_and_mint_transaction {
                match mapping_contract
                    .linkDepositHashes(*hash, bridge_and_mint_transaction.hash)
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
                                bridge_from_hash=%hash,
                                bridge_and_mint_hash=%bridge_and_mint_transaction.hash,
                                error = %e,
                                "Failed to retrieve `linkDepositHashes` receipt"
                            );
                        }
                    }
                    Err(e) => {
                        // If send failed, decrement the nonce
                        relayer_nonce.fetch_sub(1, Ordering::SeqCst);
                        warn!(
                                bridge_from_hash=%hash,
                                bridge_and_mint_hash=%bridge_and_mint_transaction.hash,
                            error = %e,
                            "Failed to link deposit hashes"
                        );
                    }
                }
                // Process next incomplete deposit hash
                continue 'hash_loop;
            }
        }
    }

    Ok(())
}
