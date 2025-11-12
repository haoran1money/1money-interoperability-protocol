use core::time::Duration;

use tracing::info;

use crate::config::Config;
use crate::incoming::recovery::get_latest_incomplete_block_number;
use crate::outgoing::recovery::get_earliest_incomplete_checkpoint_number;

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
    /// Relay Proof-of-Authority events from 1Money to the sidechain
    ProofOfAuthority {
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "10s",
            help = "Polling interval for fetching epochs (human-friendly, e.g. 10s, 1m)"
        )]
        poll_interval: Duration,
    },
    /// Relay sidechain interoperability events into 1Money
    Sidechain {
        #[arg(
            long,
            help = "Starting block number on the sidechain to scan for events (inclusive). If no value is given the number will be computed."
        )]
        from_block: Option<u64>,
    },
    /// Relay 1Money interoperability events into Sidechain
    Onemoney {
        #[arg(
            long,
            help = "Starting checkpoint number on 1Money to scan for events (inclusive)"
        )]
        start_checkpoint: Option<u64>,
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "10s",
            help = "Polling interval for fetching checkpoints (human-friendly, e.g. 10s, 1m)"
        )]
        poll_interval: Duration,
    },
}

impl Cli {
    pub async fn run(self) -> Result<(), crate::error::Error> {
        let Self { config, command } = self;

        let sidechain_relayer_nonce = config.sidechain_relayer_nonce().await?;

        match command {
            Commands::ProofOfAuthority { poll_interval } => {
                info!(
                    ?poll_interval,
                    "Relaying POA events from {} to {}",
                    config.one_money_node_url,
                    config.side_chain_node_url
                );
                crate::poa::relay_poa_events(
                    &config,
                    sidechain_relayer_nonce.clone(),
                    poll_interval,
                )
                .await?;
            }
            Commands::Sidechain { from_block } => {
                let from_block = if let Some(block_number) = from_block {
                    block_number
                } else {
                    get_latest_incomplete_block_number(&config).await?
                };
                info!(
                    ?config.interop_contract_address,
                    from_block,
                    "Clearing SC events from {} to {}",
                    config.side_chain_node_url,
                    config.one_money_node_url
                );
                crate::incoming::relay_sc_events(&config, from_block).await?;
            }
            Commands::Onemoney {
                start_checkpoint,
                poll_interval,
            } => {
                if let Some(start_checkpoint) = start_checkpoint {
                    start_checkpoint
                } else {
                    get_earliest_incomplete_checkpoint_number(&config).await?
                };
                let start_checkpoint = get_earliest_incomplete_checkpoint_number(&config).await?;
                info!(
                    start_checkpoint,
                    "Clearing 1Money events from {} to {}",
                    config.one_money_node_url,
                    config.side_chain_node_url
                );
                info!(
                    start_checkpoint,
                    "Relaying 1Money events from {} to {}",
                    config.one_money_node_url,
                    config.side_chain_node_url
                );
                crate::outgoing::stream::relay_outgoing_events(
                    &config,
                    sidechain_relayer_nonce.clone(),
                    start_checkpoint,
                    poll_interval,
                )
                .await?;
            }
        }
        Ok(())
    }
}
