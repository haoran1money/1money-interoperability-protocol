use core::future::Future;
use core::time::Duration;

use relayer::config::Config;
use relayer::incoming::relay_sc_events;
use tracing::{debug, error, warn};

pub mod account;
pub mod operator;
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
pub async fn spawn_relayer_and<F, Fut>(config: Config, work: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut relayer_task = tokio::spawn({
        let config = config.clone();
        async move {
            let relayer_result = relay_sc_events(&config, 0).await;
            if let Err(err) = &relayer_result {
                warn!(%err, "relayer side-chain event loop ended");
            }
            relayer_result
        }
    });

    let work_future = work();
    tokio::pin!(work_future);

    tokio::select! {
        outcome = &mut work_future => {
            outcome?;
            relayer_task.abort();
            match relayer_task.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(relay_err)) => {
                    error!(?relay_err, "Relayer task ended with error after test completion");
                    Err(eyre!(relay_err))
                }
                Err(join_err) if join_err.is_cancelled() => {
                    debug!("Relayer task cancelled after test completion");
                    Ok(())
                }
                Err(join_err) => {
                    error!(?join_err, "Relayer task join failed after test completion");
                    Err(eyre!(join_err))
                }
            }
        }
        relayer_result = &mut relayer_task => {
            match relayer_result {
                Ok(Ok(())) => {
                    warn!("Relayer task completed before test steps finished");
                    Err(eyre!("Relayer task completed before test steps finished"))
                }
                Ok(Err(relay_err)) => {
                    error!(?relay_err, "Relayer task errored before test steps finished");
                    Err(eyre!(relay_err))
                }
                Err(join_err) if join_err.is_cancelled() => {
                    warn!(?join_err, "Relayer task cancelled before test steps finished");
                    Err(eyre!(join_err))
                }
                Err(join_err) => {
                    error!(?join_err, "Relayer task join failed before test steps finished");
                    Err(eyre!(join_err))
                }
            }
        }
    }
}
