use core::time::Duration;

use futures::TryStreamExt;
use humantime::format_duration;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::onemoney::stream::transaction_stream;
use crate::outgoing::error::Error;
use crate::outgoing::relay::{process_burn_and_bridge_transactions, process_checkpoint_info};

pub async fn relay_outgoing_events(
    config: &Config,
    start_checkpoint: u64,
    poll_interval: Duration,
) -> Result<(), Error> {
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

    // TODO: Checkpoints will be replaced by certified transactions
    info!(
        "Fetching checkpoints every {}",
        format_duration(poll_interval)
    );

    let mut transaction_stream = transaction_stream(config, start_checkpoint, poll_interval);

    while let Some((current_checkpoint_id, transactions)) = transaction_stream.try_next().await? {
        info!(
            transactions = transactions.len(),
            "Received transactions from stream"
        );
        debug!(?transactions, "transactions details");

        process_checkpoint_info(config, current_checkpoint_id, transactions.len() as u32).await?;

        for tx in transactions {
            process_burn_and_bridge_transactions(config, tx)
                .await
                .inspect_err(|err| {
                    error!("Failed processing burn and bridge transaction stream: {err:?}");
                })?;
        }
    }

    Ok(())
}
