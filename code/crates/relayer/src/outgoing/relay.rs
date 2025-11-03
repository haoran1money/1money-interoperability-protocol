use alloy_provider::ProviderBuilder;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::{Client, Transaction, TxPayload};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::onemoney::transaction::get_transactions_from_checkpoint;
use crate::outgoing::error::Error;

/// Process burn and bridge transactions by invoking the bridgeTo method on the OMInterop contract.
/// This function expects a TokenBurnAndBridge transaction and extracts necessary details to call the contract method.
pub async fn process_burn_and_bridge_transactions(
    config: &Config,
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

    let client = Client::custom(config.one_money_node_url.to_string())?;
    // TODO: Use get_account_bbnonce() once implemented
    let bbnonce = client.get_account_nonce(sender).await?;

    let tx_receipt = contract
        .bridgeTo(
            sender,
            bbnonce.nonce,
            destination_address.parse()?,
            value.parse()?,
            destination_chain_id.try_into()?,
            escrow_fee.parse()?,
            token,
            checkpoint_number,
        )
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

/// Process all burn and bridge transactions found in a specific checkpoint.
pub async fn process_checkpoint_number(
    config: &Config,
    checkpoint_number: u64,
) -> Result<(), Error> {
    let burn_and_bridge_txs = get_transactions_from_checkpoint(
        config.one_money_node_url.to_string(),
        checkpoint_number,
        |tx| matches!(tx.data, TxPayload::TokenBurnAndBridge { .. }),
    )
    .await?;

    debug!(
        "Found {} burn and bridge transactions in checkpoint {}",
        burn_and_bridge_txs.len(),
        checkpoint_number
    );

    for burn_and_bridge in burn_and_bridge_txs {
        process_burn_and_bridge_transactions(config, burn_and_bridge)
            .await
            .inspect_err(|err| {
                error!("Failed processing burn and bridge transaction at checkpoint {checkpoint_number}: {err:?}");
            })?;
    }

    Ok(())
}
