use alloy_primitives::BlockNumber;
use futures::TryStreamExt;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::client::http::Client;
use tracing::{debug, info, warn};

use crate::config::Config;

pub mod error;
mod handlers;

use error::Error as IncomingError;
use handlers::Relayer1MoneyContext;

pub async fn relay_sc_events(
    config: &Config,
    from_block: BlockNumber,
) -> Result<(), IncomingError> {
    let mut sc_event_stream = onemoney_interop::event::event_stream(
        config.side_chain_node_url.clone(),
        config.interop_contract_address,
        from_block,
    )
    .await;

    let onemoney_client = Client::custom(config.one_money_node_url.to_string())?;
    let relayer_ctx =
        Relayer1MoneyContext::new(&onemoney_client, &config.relayer_private_key).await?;

    while let Some(event) = sc_event_stream.try_next().await? {
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
            continue;
        }

        let log = event.inner;

        match log.data {
            OMInterop::OMInteropEvents::OMInteropReceived(inner) => {
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

                let response = relayer_ctx
                    .handle_om_interop_received(inner, tx_hash)
                    .await?;

                info!(
                    ?block_number,
                    ?log_index,
                    ?tx_hash,
                    relayer_tx_hash = ?response.hash,
                    "Submitted bridge_and_mint transaction to 1Money"
                );
            }
            OMInterop::OMInteropEvents::OMInteropSent(inner) => {
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

                let response = relayer_ctx.handle_om_interop_sent(inner).await?;

                info!(
                    ?block_number,
                    ?log_index,
                    ?tx_hash,
                    relayer_tx_hash = ?response.hash,
                    "Submitted refund payment transaction to 1Money"
                );
            }
            OMInterop::OMInteropEvents::OperatorUpdated(inner) => {
                warn!(
                    ?block_number,
                    ?log_index,
                    ?tx_hash,
                    address = ?log.address,
                    operator = ?inner.newOperator,
                    "Ignoring OperatorUpdated event"
                );
            }
            OMInterop::OMInteropEvents::RelayerUpdated(inner) => {
                warn!(
                    ?block_number,
                    ?log_index,
                    ?tx_hash,
                    address = ?log.address,
                    relayer = ?inner.newRelayer,
                    "Ignoring RelayerUpdated event"
                );
            }
            OMInterop::OMInteropEvents::OwnershipTransferred(inner) => {
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
        }
    }

    Ok(())
}
