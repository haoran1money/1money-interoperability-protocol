use core::time::Duration;

use tracing::info;

use crate::config::Config;

#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub config: Config,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand)]
pub enum Commands {
    /// Relay POA events from onemoney endpoint to sidechain
    Poa {
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "10s",
            help = "Polling interval for fetching epochs (human-friendly, e.g. 10s, 1m)"
        )]
        poll_interval: Duration,
    },
}

impl Cli {
    pub async fn run(self) -> Result<(), crate::error::Error> {
        let Self { config, command } = self;
        match command {
            Commands::Poa { poll_interval } => {
                info!(
                    "Relaying POA events from {} to {}",
                    config.one_money_node_url, config.side_chain_node_url
                );
                crate::poa::relay_poa_events(&config, poll_interval).await?;
            }
        }
        Ok(())
    }
}
