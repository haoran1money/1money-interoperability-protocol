use core::time::Duration;

use async_stream::try_stream;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use tokio::time::interval;
use tracing::error;

use crate::onemoney::error::Error;
use crate::onemoney::types::epoch::{Epoch, RawEpoch};

pub mod error;
pub mod transaction;
pub mod types;

#[cfg(test)]
mod tests;

use tracing::{debug, info};
use url::Url;

pub const REST_API_EPOCH: &str = "v1/governances/epoch";

pub fn epoch_stream(url: Url, poll_interval: Duration) -> BoxStream<'static, Result<Epoch, Error>> {
    try_stream! {
        let request_url = url.join(REST_API_EPOCH)?;
        let client = Client::new();
        let mut interval = interval(poll_interval);
        let mut last_epoch_id = None;

        loop {
            interval.tick().await;

            let raw_epoch = client.get(request_url.clone())
                .send()
                .await
                .inspect_err(|err| error!("Failed to fetch epoch: {err}"))?
                .json::<RawEpoch>()
                .await
                .inspect_err(|err| error!("Failed to decode epoch response: {err}"))?;

            if last_epoch_id != Some(raw_epoch.epoch_id) {
                last_epoch_id = Some(raw_epoch.epoch_id);
                info!(epoch = raw_epoch.epoch_id, "New epoch received");
                yield raw_epoch.into();
            } else {
                debug!(epoch = raw_epoch.epoch_id, "No new epoch");
            }
        }
    }
    .boxed()
}
