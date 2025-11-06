use core::future::Future;
use core::time::Duration;

use relayer::config::Config;
use relayer::incoming::relay_sc_events;
use relayer::outgoing::stream::relay_outgoing_events;
use tracing::{debug, error, warn};

pub mod account;
pub mod operator;
pub mod setup;
pub mod transaction;

use color_eyre::eyre::{eyre, Result};

pub const POLL_INTERVAL: Duration = Duration::from_secs(1);
pub const MAX_DURATION: Duration = Duration::from_secs(20);

/// Polls `attempt` until it gives a value or we hit the timeout.
pub async fn poll_with_timeout<F, Fut, T>(
    operation_name: &str,
    poll_interval: Duration,
    timeout: Duration,
    mut attempt: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
{
    let mut interval = tokio::time::interval(poll_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    tokio::time::timeout(timeout, async {
        loop {
            interval.tick().await;
            if let Some(value) = attempt().await? {
                return Ok(value);
            }
        }
    })
    .await
    .map_err(|_| eyre!("{operation_name} did not complete within {:?}", timeout))?
}

/// Starts a relayer task, runs `work`, and handles whichever finishes first.
pub async fn spawn_relayer_and<F, Fut>(
    config: Config,
    start_checkpoint: u64,
    poll_interval: Duration,
    work: F,
) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut relayer_incoming_task = tokio::spawn({
        let config = config.clone();
        async move {
            let relayer_result = relay_sc_events(&config, 0).await;
            if let Err(err) = &relayer_result {
                warn!(%err, "relayer side-chain event loop ended");
            }
            relayer_result
        }
    });

    let mut relayer_outgoing_task = tokio::spawn({
        let config = config.clone();
        async move {
            let relayer_result =
                relay_outgoing_events(&config, start_checkpoint, poll_interval).await;
            if let Err(err) = &relayer_result {
                warn!(%err, "relayer 1Money event loop ended");
            }
            relayer_result
        }
    });

    let work_future = work();
    tokio::pin!(work_future);

    tokio::select! {
        outcome = &mut work_future => {
            outcome?;
            relayer_incoming_task.abort();
            relayer_outgoing_task.abort();
            match (relayer_incoming_task.await, relayer_outgoing_task.await) {
                (Ok(Ok(())), Ok(Ok(()))) => Ok(()),
                (Err(join_err), Ok(Ok(()))) if join_err.is_cancelled() => {
                    debug!("Relayer incoming task cancelled after test completion");
                    Ok(())
                }
                (Ok(Ok(())), Err(join_err)) if join_err.is_cancelled() => {
                    debug!("Relayer outgoing task cancelled after test completion");
                    Ok(())
                }
                (Err(incoming_join_err), Err(outgoing_join_err)) if incoming_join_err.is_cancelled() && outgoing_join_err.is_cancelled() => {
                    debug!("Relayer tasks cancelled after test completion");
                    Ok(())
                }
                (Ok(Err(relay_err)), Ok(Ok(()))) => {
                    error!(?relay_err, "Relayer incoming task ended with error after test completion");
                    Err(eyre!(relay_err))
                }
                (Err(join_err), Ok(Ok(()))) => {
                    error!(?join_err, "Relayer incoming task join failed after test completion");
                    Err(eyre!(join_err))
                }
                (Ok(Ok(())), Ok(Err(relay_err))) => {
                    error!(?relay_err, "Relayer outgoing task ended with error after test completion");
                    Err(eyre!(relay_err))
                }
                (Ok(Ok(())), Err(join_err)) => {
                    error!(?join_err, "Relayer outgoing task join failed after test completion");
                    Err(eyre!(join_err))
                }
                (Ok(Err(relay_err)), Err(join_err)) => {
                    error!(?relay_err, "Relayer incoming task ended with error after test completion");
                    error!(?join_err, "Relayer outgoing task join failed after test completion");
                    Err(eyre!("Incoming: {relay_err}. Outgoing: {join_err}"))
                }

                (Err(join_err), Ok(Err(relay_err)), ) => {
                    error!(?join_err, "Relayer incoming task join failed after test completion");
                    error!(?relay_err, "Relayer outgoing task ended with error after test completion");
                    Err(eyre!("Incoming: {join_err}. Outgoing: {relay_err}"))
                }
                (Ok(Err(incoming_relay_err)), Ok(Err(outgoing_relay_err))) => {
                    error!(?incoming_relay_err, ?outgoing_relay_err, "Relayer tasks ended with error after test completion");
                    Err(eyre!("Incoming: {incoming_relay_err}. Outgoing: {outgoing_relay_err}"))
                }
                (Err(incoming_join_err), Err(outgoing_join_err)) => {
                    error!(?incoming_join_err, ?outgoing_join_err, "Relayer tasks join failed after test completion");
                    Err(eyre!("Incoming: {incoming_join_err}. Outgoing: {outgoing_join_err}"))
                }
            }
        }
        relayer_result = &mut relayer_incoming_task => {
            match relayer_result {
                Ok(Ok(())) => {
                    warn!("Relayer incoming task completed before test steps finished");
                    Err(eyre!("Relayer incoming task completed before test steps finished"))
                }
                Ok(Err(relay_err)) => {
                    error!(?relay_err, "Relayer incoming task errored before test steps finished");
                    Err(eyre!(relay_err))
                }
                Err(join_err) if join_err.is_cancelled() => {
                    warn!(?join_err, "Relayer incoming task cancelled before test steps finished");
                    Err(eyre!(join_err))
                }
                Err(join_err) => {
                    error!(?join_err, "Relayer incoming task join failed before test steps finished");
                    Err(eyre!(join_err))
                }
            }
        }
        relayer_result = &mut relayer_outgoing_task => {
            match relayer_result {
                Ok(Ok(())) => {
                    warn!("Relayer outgoing task completed before test steps finished");
                    Err(eyre!("Relayer outgoing task completed before test steps finished"))
                }
                Ok(Err(relay_err)) => {
                    error!(?relay_err, "Relayer outgoing task errored before test steps finished");
                    Err(eyre!(relay_err))
                }
                Err(join_err) if join_err.is_cancelled() => {
                    warn!(?join_err, "Relayer outgoing task cancelled before test steps finished");
                    Err(eyre!(join_err))
                }
                Err(join_err) => {
                    error!(?join_err, "Relayer outgoing task join failed before test steps finished");
                    Err(eyre!(join_err))
                }
            }
        }
    }
}
