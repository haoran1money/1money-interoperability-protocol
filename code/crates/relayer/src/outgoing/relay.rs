use core::sync::atomic::Ordering;

use alloy_provider::ProviderBuilder;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::{Transaction, TxPayload};
use tracing::{debug, info};

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
        .connect_http(config.side_chain_node_url.clone());

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
        .connect_http(config.side_chain_node_url.clone());

    let contract = OMInterop::new(config.interop_contract_address, provider);

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

    let checkpoint_number = tx.checkpoint_number.ok_or(Error::MissingCheckpointNumber)?;

    // TODO: Replace this with correct `bbnonce` once it is added to the `Transaction`
    // We should validate the bbnonce from the burn transaction matches the bbnonce at side-chain.
    let bbnonce = contract
        .getLatestProcessedNonce(sender)
        .call()
        .block("pending".parse().unwrap())
        .await?;

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

    info!(?tx_receipt, "Tx receipt for bridge to");

    Ok(())
}
