use core::time::Duration;

use async_stream::stream;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use tokio::time::interval;

use crate::onemoney::error::Error;
use crate::onemoney::types::{Epoch, RawEpoch};

pub mod error;
pub mod types;

#[cfg(test)]
mod tests;

use tracing::{debug, error, info};
use url::Url;

pub const REST_API_EPOCH: &str = "v1/governances/epoch";

pub fn epoch_stream(url: Url, poll_interval: Duration) -> BoxStream<'static, Result<Epoch, Error>> {
    stream! {
        let request_url = match url.join(REST_API_EPOCH) {
            Ok(url) => url,
            Err(err) => {
                yield Err(Error::Url(err));
                return;
            }
        };

        let client = Client::new();
        let mut interval = interval(poll_interval);
        let mut last_epoch_id = None;

        loop {
            interval.tick().await;
            let response = match client.get(request_url.clone()).send().await {
                Ok(response) => response,
                Err(err) => {
                    error!("Error fetching epoch data: {:?}", err);
                    yield Err(err.into());
                    continue;
                },
            };
            let raw_epoch = match response.json::<RawEpoch>().await {
                Ok(raw) => raw,
                Err(err) => {
                    error!("Error decoding epoch data: {:?}", err);
                    yield Err(err.into());
                    continue;
                },
            };

            if last_epoch_id != Some(raw_epoch.epoch_id) {
                last_epoch_id = Some(raw_epoch.epoch_id);
                info!(epoch = raw_epoch.epoch_id, "New epoch received");
                yield Ok(raw_epoch.into());
            } else {
                debug!(epoch = raw_epoch.epoch_id, "No new epoch");
            }
        }
    }
    .boxed()
}
