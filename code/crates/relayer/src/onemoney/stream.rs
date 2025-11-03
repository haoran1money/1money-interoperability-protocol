use core::time::Duration;

use async_stream::try_stream;
use futures::stream::BoxStream;
use futures::StreamExt;
use onemoney_protocol::{Transaction, TxPayload};
use tokio::time::interval;
use tracing::{debug, info};
use url::Url;

use crate::onemoney::error::Error;
use crate::onemoney::transaction::get_transactions_from_checkpoint;

pub fn transaction_stream(
    url: Url,
    start_checkpoint: u64,
    poll_interval: Duration,
) -> BoxStream<'static, Result<Vec<Transaction>, Error>> {
    try_stream! {
        let mut interval = interval(poll_interval);
        let mut current_checkpoint_id = start_checkpoint;

        loop {
            interval.tick().await;

            // TODO: This will be replaced by certified transactions
            match get_transactions_from_checkpoint(url.to_string(), current_checkpoint_id, |tx| {
                matches!(tx.data, TxPayload::TokenBurnAndBridge { .. })
            }).await {
                Ok(transactions) => {
                    info!(checkpoint = current_checkpoint_id, "BurnAndBridge transactions extracted");

                    current_checkpoint_id += 1;

                    yield transactions;
                },
                Err(err) => {
                    // If the checkpoint doesn't exist it will return a 404 error, we just log and try again later
                    debug!("Failed to fetch checkpoint will try again: {err}");
                }
            }
        }
    }
    .boxed()
}
