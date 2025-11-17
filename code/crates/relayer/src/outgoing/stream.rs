use core::time::Duration;

use futures::TryStreamExt;
use humantime::format_duration;
use tracing::{debug, error, info};

use crate::config::{Config, RelayerNonce};
use crate::onemoney::stream::transaction_stream;
use crate::outgoing::error::Error;
use crate::outgoing::relay::{process_burn_and_bridge_transactions, process_checkpoint_info};

pub async fn relay_outgoing_events(
    config: &Config,
    relayer_nonce: RelayerNonce,
    start_checkpoint: u64,
    poll_interval: Duration,
) -> Result<(), Error> {
    info!(
        url = %config.one_money_node_url,
        "Connecting to onemoney",
    );
    info!(
        url = %config.side_chain_http_url,
        "Connecting to sidechain",
    );
    info!(
        relayer_address = %config.relayer_private_key.address(),
    );

    // TODO: Checkpoints will be replaced by certified transactions
    info!(
        interval = %format_duration(poll_interval),
        "Fetching checkpoints",
    );

    let mut transaction_stream = transaction_stream(config, start_checkpoint, poll_interval);

    while let Some((current_checkpoint_id, transactions)) = transaction_stream.try_next().await? {
        debug!(
            transactions = transactions.len(),
            "Processing BurnAndBridge transactions from stream"
        );
        debug!(?transactions, "transactions details");

        process_checkpoint_info(
            config,
            relayer_nonce.clone(),
            current_checkpoint_id,
            transactions.len() as u32,
        )
        .await?;

        for tx in transactions {
            process_burn_and_bridge_transactions(config, relayer_nonce.clone(), tx)
                .await
                .inspect_err(|err| {
                    error!(?err, "Failed processing burn and bridge transaction stream");
                })?;
        }
    }

    Ok(())
}
