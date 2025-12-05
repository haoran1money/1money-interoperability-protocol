use core::sync::atomic::Ordering;

use alloy_primitives::Bytes;
use alloy_provider::ProviderBuilder;
use onemoney_interop::contract::{OMInterop, TxHashMapping};
use onemoney_protocol::{Client, Transaction, TxPayload};
use tracing::{debug, warn};

use crate::config::{Config, RelayerNonce};
use crate::outgoing::error::Error;

pub async fn process_checkpoint_info(
    config: &Config,
    relayer_nonce: RelayerNonce,
    current_checkpoint_id: u64,
    transaction_count: u32,
) -> Result<(), Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());

    let contract = OMInterop::new(config.interop_contract_address, provider);

    let tx_receipt = contract
        .updateCheckpointInfo(current_checkpoint_id, transaction_count)
        .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
        .send()
        .await
        .map(Ok)
        .or_else(|e| {
            e.try_decode_into_interface_error::<OMInterop::OMInteropErrors>()
                .map(Err)
        })?
        .map_err(Error::ContractReverted)?
        .get_receipt()
        .await?;

    debug!(
        ?tx_receipt,
        "Successfully updated checkpoint {current_checkpoint_id} tally information"
    );

    Ok(())
}

/// Process burn and bridge transactions by invoking the bridgeTo method on the OMInterop contract.
/// This function expects a TokenBurnAndBridge transaction and extracts necessary details to call the contract method.
pub async fn process_burn_and_bridge_transactions(
    config: &Config,
    relayer_nonce: RelayerNonce,
    tx: Transaction,
) -> Result<(), Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());

    let contract = OMInterop::new(config.interop_contract_address, provider.clone());

    let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider);

    let client = Client::custom(config.one_money_node_url.to_string())?;

    let TxPayload::TokenBurnAndBridge {
        value,
        sender,
        destination_chain_id,
        destination_address,
        escrow_fee,
        bridge_metadata: _,
        token,
    } = tx.data
    else {
        return Err(Error::Generic(
            "Expected TokenBurnAndBridge transaction".to_string(),
        ));
    };

    debug!(burnAndBridgeHas = %tx.hash, "Will register withdrawal transaction hash");

    match mapping_contract
        .registerWithdrawal(tx.hash)
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
                    %tx.hash,
                    error = %e,
                    "Failed to retrieve withdrawal transaction hash link receipt"
                );
            }
        }
        Err(e) => {
            // If send failed, decrement the nonce
            relayer_nonce.fetch_sub(1, Ordering::SeqCst);
            warn!(
                %tx.hash,
                error = %e,
                "Failed to register withdrawal transaction hash"
            );
        }
    }

    let checkpoint_number = tx.checkpoint_number.ok_or(Error::MissingCheckpointNumber)?;

    let burn_and_bridge_receipt = client
        .get_transaction_receipt_by_hash(&tx.hash.to_string())
        .await?;

    // The bbnonce in the BurnAndBridge receipt is the account's next nonce,
    // so we subtract 1 to get the current nonce.
    let bbnonce = burn_and_bridge_receipt
        .success_info
        .ok_or_else(|| {
            Error::Generic(format!(
                "missing `success_info` in BurnAndBridge receipt for transaction `{}`",
                tx.hash
            ))
        })?
        .bridge_info
        .ok_or_else(|| {
            Error::Generic(format!(
                "missing `bridge_info` in BurnAndBridge receipt for transaction `{}`",
                tx.hash
            ))
        })?
        .bbnonce
        - 1;

    // TODO: Handle bridgeData when it is added to the TokenBurnAndBridge.
    // For now, we pass an empty bytes array.
    let bridge_data = Bytes::new();

    let latest_bb = contract.getLatestProcessedNonce(sender).call().await?;

    if latest_bb > bbnonce {
        warn!(burn_and_bridge_hash=%tx.hash, "Skipping BurnAndBridge as it was already processed");
        return Ok(());
    }

    let tx_receipt = contract
        .bridgeTo(
            sender,
            bbnonce,
            destination_address.parse()?,
            value.parse()?,
            destination_chain_id.try_into()?,
            escrow_fee.parse()?,
            token,
            checkpoint_number,
            bridge_data,
            tx.hash,
        )
        .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
        .send()
        .await
        .map(Ok)
        .or_else(|e| {
            e.try_decode_into_interface_error::<OMInterop::OMInteropErrors>()
                .map(Err)
        })?
        .map_err(Error::ContractReverted)?
        .get_receipt()
        .await?;

    debug!(?tx_receipt, "Tx receipt for bridge to");

    debug!(burnAndBridgeHas = %tx.hash, bridgeToHash = %tx_receipt.transaction_hash, "Will link withdrawal transaction hash");

    match mapping_contract
        .linkWithdrawalHashes(tx.hash, tx_receipt.transaction_hash)
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
                    burn_and_bridge_hash=%tx.hash,
                    bridge_to_hash=%tx_receipt.transaction_hash,
                    error = %e,
                    "Failed to retrieve `linkWithdrawalHashes` receipt"
                );
            }
        }
        Err(e) => {
            // If send failed, decrement the nonce
            relayer_nonce.fetch_sub(1, Ordering::SeqCst);
            warn!(
                    burn_and_bridge_hash=%tx.hash,
                    bridge_to_hash=%tx_receipt.transaction_hash,
                error = %e,
                "Failed to link withdrawal hashes"
            );
        }
    }

    Ok(())
}
