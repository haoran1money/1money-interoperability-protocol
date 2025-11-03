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
    SideChain {
        #[arg(
            long,
            default_value_t = 0_u64,
            help = "Starting block number on the sidechain to scan for events (inclusive)"
        )]
        from_block: u64,
    },
    /// Relay 1Money interoperability events into Sidechain
    OneMoney {
        #[arg(
            long,
            help = "Starting checkpoint number on 1Money to scan for events (inclusive)"
        )]
        start_checkpoint: u64,
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
        match command {
            Commands::ProofOfAuthority { poll_interval } => {
                info!(
                    ?poll_interval,
                    "Relaying POA events from {} to {}",
                    config.one_money_node_url,
                    config.side_chain_node_url
                );
                crate::poa::relay_poa_events(&config, poll_interval).await?;
            }
            Commands::SideChain { from_block } => {
                info!(
                    ?config.interop_contract_address,
                    from_block,
                    "Relaying SC events from {} to {}",
                    config.side_chain_node_url,
                    config.one_money_node_url
                );
                crate::incoming::relay_sc_events(&config, from_block).await?;
            }
            Commands::OneMoney {
                start_checkpoint,
                poll_interval,
            } => {
                info!(
                    start_checkpoint,
                    "Relaying 1Money events from {} to {}",
                    config.one_money_node_url,
                    config.side_chain_node_url
                );
                crate::outgoing::stream::relay_outgoing_events(
                    &config,
                    start_checkpoint,
                    poll_interval,
                )
                .await?;
            }
        }
        Ok(())
    }
}
