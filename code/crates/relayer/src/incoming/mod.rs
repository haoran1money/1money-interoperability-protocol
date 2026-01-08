use alloy_primitives::BlockNumber;
use alloy_rpc_types_eth::Log;
use futures::TryStreamExt;
use onemoney_interop::contract::OMInterop::{self, OMInteropEvents};
use onemoney_protocol::client::http::Client;
use tracing::{debug, info, warn};

use crate::config::{Config, RelayerNonce};

pub mod error;
mod handlers;
pub mod recovery;

use error::Error as IncomingError;
use handlers::Relayer1MoneyContext;

pub async fn relay_incoming_events(
    config: &Config,
    relayer_nonce: RelayerNonce,
    from_block: BlockNumber,
) -> Result<(), IncomingError> {
    let sc_event_stream = onemoney_interop::event::event_stream(
        config.side_chain_http_url.clone(),
        config.side_chain_ws_url.clone(),
        config.interop_contract_address,
        from_block,
    )
    .await;

    sc_event_stream
        .map_err(IncomingError::from)
        .try_for_each(|event| async { process_event(event, config, relayer_nonce.clone()).await })
        .await?;

    Ok(())
}

pub async fn process_event(
    event: Log<OMInteropEvents>,
    config: &Config,
    relayer_nonce: RelayerNonce,
) -> Result<(), IncomingError> {
    let onemoney_client = Client::custom(config.one_money_node_url.to_string())?;
    let relayer_ctx =
        Relayer1MoneyContext::new(&onemoney_client, &config.relayer_private_key).await?;

    let block_number = event
        .block_number
        .ok_or(IncomingError::MissingBlockNumber)?;
    let log_index = event.log_index.ok_or(IncomingError::MissingLogIndex)?;
    let tx_hash = event
        .transaction_hash
        .ok_or(IncomingError::MissingTransactionHash)?;
    if event.removed {
        debug!(
            ?block_number,
            ?log_index,
            ?tx_hash,
            "Skipping removed OMInterop event from stream"
        );
        return Ok(());
    }

    let log = event.inner;

    match log.data {
        OMInteropEvents::OMInteropReceived(inner) => {
            info!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                nonce = inner.nonce,
                to = ?inner.to,
                amount = %inner.amount,
                om_token = ?inner.omToken,
                src_chain_id = inner.srcChainId,
                "Handling OMInteropReceived event"
            );

            if relayer_ctx.should_process_nonce(inner.nonce).await? {
                let response = relayer_ctx
                    .handle_om_interop_received(config, relayer_nonce.clone(), inner, tx_hash)
                    .await?;

                info!(
                    ?block_number,
                    ?log_index,
                    ?tx_hash,
                    relayer_tx_hash = ?response,
                    "Submitted bridge_and_mint transaction to 1Money"
                );
            }
        }
        OMInteropEvents::OMInteropSent(inner) => {
            info!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                nonce = inner.nonce,
                from = ?inner.from,
                refund = %inner.refundAmount,
                om_token = ?inner.omToken,
                dst_chain_id = inner.dstChainId,
                source_hash = ?inner.sourceHash,
                "Handling OMInteropSent event"
            );

            if inner.refundAmount.is_zero() {
                warn!(
                        ?block_number,
                        ?log_index,
                        ?tx_hash,
                        "OMInteropSent event with zero refund amount. Can't skip because of nonce tracking.",
                    );
            }

            if relayer_ctx.should_process_nonce(inner.nonce).await? {
                let response = relayer_ctx
                    .handle_om_interop_sent(config, relayer_nonce.clone(), inner)
                    .await?;

                info!(
                    ?block_number,
                    ?log_index,
                    ?tx_hash,
                    relayer_tx_hash = ?response,
                    "Submitted refund payment transaction to 1Money"
                );
            }
        }
        OMInteropEvents::OperatorUpdated(inner) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                operator = ?inner.newOperator,
                "Ignoring OperatorUpdated event"
            );
        }
        OMInteropEvents::RelayerUpdated(inner) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                relayer = ?inner.newRelayer,
                "Ignoring RelayerUpdated event"
            );
        }
        OMInteropEvents::OwnershipTransferred(inner) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                previous_owner = ?inner.previousOwner,
                new_owner = ?inner.newOwner,
                "Ignoring OwnershipTransferred event"
            );
        }
        OMInteropEvents::RateLimitsChanged(inner) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                token = ?inner.token,
                limit = ?inner.limit,
                window = ?inner.window,
                "Ignoring RateLimitsChanged event"
            );
        }
        OMInteropEvents::Initialized(_) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                "Ignoring Initialized event"
            );
        }
        OMInteropEvents::Upgraded(_) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                "Ignoring Upgraded event"
            );
        }
        OMInterop::OMInteropEvents::PriceOracleUpdated(_) => {
            warn!(
                ?block_number,
                ?log_index,
                ?tx_hash,
                address = ?log.address,
                "Ignoring PriceOracleUpdated event"
            );
        }
    }
    Ok(())
}
