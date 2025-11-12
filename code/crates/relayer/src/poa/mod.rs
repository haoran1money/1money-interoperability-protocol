use core::time::Duration;

use futures::StreamExt;
use humantime::format_duration;
use tracing::{debug, error, info};
use validator_manager::ValidatorManager::ValidatorInfo;

use crate::config::{Config, RelayerNonce};
use crate::poa::error::Error as PoaError;

pub mod error;

pub async fn relay_poa_events(
    config: &Config,
    relayer_nonce: RelayerNonce,
    poll_interval: Duration,
) -> Result<(), PoaError> {
    info!(
        "Connecting to onemoney endpoint: {}",
        config.one_money_node_url
    );
    info!(
        "Connecting to sidechain endpoint: {}",
        config.side_chain_node_url
    );
    info!(
        "Using relayer address: {}",
        config.relayer_private_key.address()
    );
    info!("Fetching epochs every {}", format_duration(poll_interval));

    let mut epoch_stream =
        crate::onemoney::epoch_stream(config.one_money_node_url.clone(), poll_interval);
    while let Some(epoch_result) = epoch_stream.next().await {
        match epoch_result {
            Ok(epoch) => {
                info!(epoch = epoch.epoch_id, "Updating validator set");
                debug!(?epoch, "Epoch details");
                let sidechain_validator_info = epoch
                    .validator_set
                    .members
                    .into_iter()
                    .map(ValidatorInfo::try_from)
                    .collect::<Result<Vec<_>, _>>()?;

                if let Err(err) = crate::sidechain::process_new_validator_set(
                    config,
                    relayer_nonce.clone(),
                    sidechain_validator_info,
                )
                .await
                {
                    error!("Failed updating validator set: {:?}", err);
                }
            }
            Err(e) => {
                error!("Error receiving epoch: {:?}", e);
            }
        }
    }

    Ok(())
}
