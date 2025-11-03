use core::time::Duration;

use futures::TryStreamExt;
use humantime::format_duration;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::onemoney::stream::transaction_stream;
use crate::outgoing::error::Error;
use crate::outgoing::relay::process_burn_and_bridge_transactions;

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

    let mut transaction_stream = transaction_stream(
        config.one_money_node_url.clone(),
        start_checkpoint,
        poll_interval,
    );

    while let Some(transactions) = transaction_stream.try_next().await? {
        info!(
            transactions = transactions.len(),
            "Received transactions from stream"
        );
        debug!(?transactions, "transactions details");

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
