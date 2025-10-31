use alloy_primitives::{Address, BlockNumber};
use futures::TryStreamExt;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::client::http::Client;
use tracing::{debug, info, warn};

use crate::config::Config;

pub mod error;
mod handlers;

use handlers::{handle_om_interop_received, handle_om_interop_sent};

pub async fn relay_sc_events(
    config: &Config,
    interop_contract_address: Address,
    from_block: BlockNumber,
) -> Result<(), error::Error> {
    let mut sc_event_stream = onemoney_interop::event::event_stream(
        config.side_chain_node_url.clone(),
        interop_contract_address,
        from_block,
    )
    .await;

    let onemoney_client = Client::custom(config.one_money_node_url.to_string())?;

    while let Some(event) = sc_event_stream.try_next().await? {
        let block_number = event.block_number;
        let log_index = event.log_index;
        let tx_hash = event.transaction_hash;
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
                    "Handling OMInteropReceived event"
                );
                handle_om_interop_received(
                    &onemoney_client,
                    inner.nonce,
                    inner.to,
                    inner.amount,
                    inner.omToken,
                )
                .await?;
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
                    "Handling OMInteropSent event"
                );
                handle_om_interop_sent(
                    &onemoney_client,
                    inner.nonce,
                    inner.from,
                    inner.refundAmount,
                    inner.omToken,
                )
                .await?;
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
