use core::time::Duration;

use alloy_primitives::B256;
use async_stream::try_stream;
use futures::stream::BoxStream;
use futures::{SinkExt, StreamExt};
use onemoney_protocol::{Transaction, TxPayload};
use serde_json::json;
use tokio::time::interval;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::onemoney::error::Error;
use crate::onemoney::transaction::get_transactions_from_checkpoint;
use crate::onemoney::types::transaction::CertifiedTransaction;

pub fn transaction_stream_from_checkpoint(
    config: &Config,
    start_checkpoint: u64,
    poll_interval: Duration,
) -> BoxStream<'static, Result<(u64, Vec<Transaction>), Error>> {
    let config = config.clone();

    try_stream! {
        let mut interval = interval(poll_interval);
        let mut current_checkpoint_id = start_checkpoint;

        loop {
            interval.tick().await;

            // TODO: This will be replaced by certified transactions
            match get_transactions_from_checkpoint(config.one_money_node_url.to_string(), current_checkpoint_id, |tx| {
                matches!(tx.data, TxPayload::TokenBurnAndBridge { .. })
            }).await {
                Ok(transactions) => {
                    if transactions.is_empty() {
                        debug!(
                            checkpoint = current_checkpoint_id,
                            "No BurnAndBridge transactions in this checkpoint, skipping"
                        );
                    } else {
                        info!(
                            count = transactions.len(),
                            checkpoint = current_checkpoint_id,
                            "Found BurnAndBridge transactions",
                        );
                        debug!(?transactions, "BurnAndBridge transactions details");
                    }

                    yield (current_checkpoint_id, transactions);

                    current_checkpoint_id += 1;
                },
                Err(err) => {
                    // If the checkpoint doesn't exist it will return a 404 error, we just log and try again later
                    debug!(%err, "Failed to fetch checkpoint will try again");
                }
            }
        }
    }
    .boxed()
}

pub fn certified_transaction_stream(
    config: &Config,
) -> BoxStream<'static, Result<(TxPayload, B256), Error>> {
    let config = config.clone();

    try_stream! {
        let raw_ws = tokio_tungstenite::connect_async(&config.one_money_ws_url.to_string()).await;

        let (mut ws, _resp) = raw_ws.unwrap();

        let subscribe = json!({
            "id": 1,
            "method": "SUBSCRIBE",
            "stream": { "name": "CERTIFIED_TRANSACTIONS", "full": true }
        });

        ws.send(Message::Text(subscribe.to_string().into()))
            .await
            .unwrap();

        info!("send subscription: {}", config.one_money_ws_url);

        // Read messages forever
        while let Some(msg) = ws.next().await {
            let msg = msg.unwrap();
            match msg {
                Message::Text(raw_tx) => {
                    match serde_json::from_str::<CertifiedTransaction>(&raw_tx) {
                        Ok(certified_transaction) => {
                        let tx = certified_transaction.get_transaction_envelope().to_tx_payload();
                        if matches!(tx, TxPayload::TokenBurnAndBridge { .. }) {
                            yield (tx.clone(), certified_transaction.result.tx_hash);
                        }
                    }
                        Err(err) => {
                            warn!("failed to deserialize certified transaction: {err:?}");
                        }
                    }
                }
                Message::Close(frame) => {
                    warn!("Certified transactions websocket stream closed: {frame:?}");
                    break;
                }
                _ => {}
            }
        }
    }
    .boxed()
}
