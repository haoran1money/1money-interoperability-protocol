use core::time::Duration;

use futures::future::{try_join, try_join5};
use futures::TryFutureExt;
use humantime::format_duration;
use tracing::info;

use crate::config::Config;
use crate::error::Error as CliError;
use crate::incoming::recovery::{
    get_latest_incomplete_block_number, recover_incomplete_deposit_hash_mapping,
    relay_incoming_events_from_blocks,
};
use crate::incoming::relay_incoming_events;
use crate::outgoing::recovery::{
    get_earliest_incomplete_checkpoint_number, recover_incomplete_withdrawals_hash_mapping,
};
use crate::outgoing::stream::{relay_outgoing_events, relay_outgoing_events_from_checkpoints};
use crate::poa::relay_poa_events;

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
        #[arg(
            long,
            help = "Starting checkpoint for Tx Hash Mapping recovery (inclusive). Defaults to 0"
        )]
        start_checkpoint_hash_mapping_recovery: Option<u64>,
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "10s",
            help = "Interval of transaction clearing on the sidechain (human-friendly, e.g. 10s, 1m). Default to 10s."
        )]
        clearing_poll_interval: Duration,
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
            default_value = "1s",
            help = "Polling interval for fetching checkpoints for interval clearing (human-friendly, e.g. 10s, 1m)"
        )]
        clearing_poll_interval: Duration,
        #[arg(
            long,
            help = "Starting checkpoint for Tx Hash Mapping recovery (inclusive). Defaults to 0"
        )]
        start_checkpoint_hash_mapping_recovery: Option<u64>,
        #[arg(
            long,
            help = "Starting block for Tx Hash Mapping recovery (inclusive). Defaults to 0"
        )]
        start_block_hash_mapping_recovery: Option<u64>,
    },
    /// Relay events from both sides concurrently
    All {
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "1s",
            help = "Polling interval for fetching epochs (human-friendly, e.g. 10s, 1m)"
        )]
        poa_poll_interval: Duration,
        #[arg(
            long,
            help = "Starting block number on the sidechain to scan for events (inclusive)"
        )]
        from_block: Option<u64>,
        #[arg(
            long,
            help = "Starting checkpoint number on 1Money to scan for events (inclusive)"
        )]
        start_checkpoint: Option<u64>,
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "1s",
            help = "Polling interval for fetching checkpoints for interval clearing (human-friendly, e.g. 10s, 1m)"
        )]
        one_money_clearing_poll_interval: Duration,
        #[arg(
            long,
            help = "Starting checkpoint for Tx Hash Mapping recovery (inclusive). Defaults to 0"
        )]
        start_checkpoint_hash_mapping_recovery: Option<u64>,
        #[arg(
            long,
            help = "Starting block for Tx Hash Mapping recovery (inclusive). Defaults to 0"
        )]
        start_block_hash_mapping_recovery: Option<u64>,
        #[arg(
            long,
            value_parser = humantime::parse_duration,
            default_value = "10s",
            help = "Interval of transaction clearing on the sidechain (human-friendly, e.g. 10s, 1m). Default to 10s."
        )]
        sidechain_clearing_poll_interval: Duration,
    },
}

impl Cli {
    pub async fn run(self) -> Result<(), CliError> {
        let Self { config, command } = self;

        let sidechain_relayer_nonce = config.sidechain_relayer_nonce().await?;

        match command {
            Commands::ProofOfAuthority { poll_interval } => {
                info!(
                    poll_interval = %format_duration(poll_interval),
                    from = %config.one_money_node_url,
                    to = %config.side_chain_http_url,
                    "Relaying POA events",
                );
                relay_poa_events(&config, sidechain_relayer_nonce.clone(), poll_interval).await?;
            }
            Commands::Sidechain {
                from_block,
                start_checkpoint_hash_mapping_recovery,
                clearing_poll_interval,
            } => {
                recover_incomplete_deposit_hash_mapping(
                    &config,
                    sidechain_relayer_nonce.clone(),
                    start_checkpoint_hash_mapping_recovery,
                )
                .await?;
                let from_block = if let Some(block_number) = from_block {
                    block_number
                } else {
                    get_latest_incomplete_block_number(&config).await?
                };
                info!(
                    ?config.interop_contract_address,
                    from_block,
                    "Clearing SC events from {} to {}",
                    config.side_chain_http_url,
                    config.one_money_node_url
                );
                info!(
                    %config.interop_contract_address,
                    from_block,
                    clearing_poll_interval = %format_duration(clearing_poll_interval),
                    from = %config.side_chain_http_url,
                    to = %config.one_money_node_url,
                    "Relaying SC events",
                );
                try_join(
                    relay_incoming_events(&config, sidechain_relayer_nonce.clone(), from_block),
                    relay_incoming_events_from_blocks(
                        from_block,
                        &config,
                        sidechain_relayer_nonce.clone(),
                        clearing_poll_interval,
                    ),
                )
                .await?;
            }
            Commands::Onemoney {
                start_checkpoint,
                clearing_poll_interval,
                start_checkpoint_hash_mapping_recovery,
                start_block_hash_mapping_recovery,
            } => {
                recover_incomplete_withdrawals_hash_mapping(
                    &config,
                    sidechain_relayer_nonce.clone(),
                    start_checkpoint_hash_mapping_recovery,
                    start_block_hash_mapping_recovery,
                )
                .await?;
                let start_checkpoint = if let Some(start_checkpoint) = start_checkpoint {
                    start_checkpoint
                } else {
                    get_earliest_incomplete_checkpoint_number(&config).await?
                };
                info!(
                    start_checkpoint,
                    "Clearing 1Money events from {} to {}",
                    config.one_money_node_url,
                    config.side_chain_http_url
                );
                info!(
                    start_checkpoint,
                    clearing_poll_interval = %format_duration(clearing_poll_interval),
                    from = %config.one_money_node_url,
                    to = %config.side_chain_http_url,
                    "Relaying 1Money events",
                );
                try_join(
                    relay_outgoing_events(&config, sidechain_relayer_nonce.clone()),
                    relay_outgoing_events_from_checkpoints(
                        &config,
                        sidechain_relayer_nonce.clone(),
                        start_checkpoint,
                        clearing_poll_interval,
                    ),
                )
                .await?;
            }

            Commands::All {
                poa_poll_interval,
                from_block,
                start_checkpoint,
                one_money_clearing_poll_interval,
                start_checkpoint_hash_mapping_recovery,
                start_block_hash_mapping_recovery,
                sidechain_clearing_poll_interval,
            } => {
                recover_incomplete_deposit_hash_mapping(
                    &config,
                    sidechain_relayer_nonce.clone(),
                    start_checkpoint_hash_mapping_recovery,
                )
                .await?;
                recover_incomplete_withdrawals_hash_mapping(
                    &config,
                    sidechain_relayer_nonce.clone(),
                    start_checkpoint_hash_mapping_recovery,
                    start_block_hash_mapping_recovery,
                )
                .await?;
                let start_checkpoint = if let Some(start_checkpoint) = start_checkpoint {
                    start_checkpoint
                } else {
                    get_earliest_incomplete_checkpoint_number(&config).await?
                };

                let from_block = if let Some(block_number) = from_block {
                    block_number
                } else {
                    get_latest_incomplete_block_number(&config).await?
                };

                info!(
                    start_checkpoint,
                    from_block,
                    poa_poll_interval = %format_duration(poa_poll_interval),
                    sidechain_clearing_poll_interval = %format_duration(sidechain_clearing_poll_interval),
                    one_money_clearing_poll_interval = %format_duration(one_money_clearing_poll_interval),
                    onemoney_url = %config.one_money_node_url,
                    sidechain_url = %config.side_chain_http_url,
                    "Relaying all flows",
                );
                try_join5(
                    relay_poa_events(&config, sidechain_relayer_nonce.clone(), poa_poll_interval)
                        .map_err(CliError::from),
                    relay_incoming_events(&config, sidechain_relayer_nonce.clone(), from_block)
                        .map_err(CliError::from),
                    relay_incoming_events_from_blocks(
                        from_block,
                        &config,
                        sidechain_relayer_nonce.clone(),
                        sidechain_clearing_poll_interval,
                    )
                    .map_err(CliError::from),
                    relay_outgoing_events(&config, sidechain_relayer_nonce.clone())
                        .map_err(CliError::from),
                    relay_outgoing_events_from_checkpoints(
                        &config,
                        sidechain_relayer_nonce.clone(),
                        start_checkpoint,
                        one_money_clearing_poll_interval,
                    )
                    .map_err(CliError::from),
                )
                .await?;
            }
        }
        Ok(())
    }
}
