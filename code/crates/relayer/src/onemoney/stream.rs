use core::time::Duration;

use async_stream::try_stream;
use futures::stream::BoxStream;
use futures::StreamExt;
use onemoney_protocol::{Transaction, TxPayload};
use tokio::time::interval;
use tracing::{debug, info};

use crate::config::Config;
use crate::onemoney::error::Error;
use crate::onemoney::transaction::get_transactions_from_checkpoint;

pub fn transaction_stream(
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
