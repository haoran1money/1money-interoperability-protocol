use alloy_primitives::Address;
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types_eth::{Filter, Log as RpcLog};
use alloy_sol_types::SolEventInterface;
use async_stream::try_stream;
use futures::stream::BoxStream;
use futures::StreamExt;
use url::Url;

use crate::contract::OMInterop;
use crate::error::Error as OMInteropError;

/// Convenience alias for decoded OMInterop logs.
pub type OMInteropLog = RpcLog<OMInterop::OMInteropEvents>;

const MAX_BLOCK_RANGE: u64 = 100_000;

/// Creates an async stream of OMInterop events starting at `from_block`.
///
/// The stream first yields any historical events since `from_block`, then
/// continues polling the node for new events by subscribing to logs over WebSocket.
pub async fn event_stream(
    http_endpoint: Url,
    ws_endpoint: Url,
    contract: Address,
    from_block: u64,
) -> BoxStream<'static, Result<OMInteropLog, OMInteropError>> {
    try_stream! {
        let ws = WsConnect::new(ws_endpoint);
        let http_provider = ProviderBuilder::new().connect_http(http_endpoint);
        let ws_provider = ProviderBuilder::new().connect_ws(ws).await?;

        let mut last_position = from_block
            .checked_sub(1)
            .map(|block| (block, u64::MAX));

        let live_filter = Filter::new().address(contract);
        let mut live_stream = ws_provider
            .subscribe_logs(&live_filter)
            .await?
            .into_stream();

        let latest_block = http_provider
            .get_block_number()
            .await?;

        // Keep a small overlap so recent logs still surface through the live stream.
        // Five blocks is enough cushion for the reorg depths we expect.
        let live_start = latest_block.saturating_add(5);

        let mut start = from_block;

        // Get chunks of 99_999 blocks to avoid error:
        // query exceeds block range 100_000
        while start <= live_start {
            let end = core::cmp::min(start + MAX_BLOCK_RANGE - 1, live_start);

            let history_filter = Filter::new()
                .address(contract)
                .select(start..=end);

            let historical = http_provider.get_logs(&history_filter).await?;

            let mut decoded = historical
                .into_iter()
                .map(decode_event)
                .collect::<Result<Vec<_>, _>>()?;

            decoded.sort_by_key(|log| {
                (
                    log.block_number.unwrap_or(u64::MAX),
                    log.log_index.unwrap_or(u64::MAX),
                )
            });

            for log in decoded {
                if let Some(position) = log_position(&log) {
                    last_position = Some(position);
                }
                yield log;
            }

            start = end + 1;
        }

        while let Some(log) = live_stream.next().await {
            let decoded = decode_event(log)?;
            if should_emit(log_position(&decoded), &mut last_position) {
                yield decoded;
            }
        }
    }
    .boxed()
}

pub fn decode_event(log: RpcLog) -> Result<OMInteropLog, OMInteropError> {
    let RpcLog {
        inner,
        block_hash,
        block_number,
        block_timestamp,
        transaction_hash,
        transaction_index,
        log_index,
        removed,
    } = log;

    Ok(RpcLog {
        inner: OMInterop::OMInteropEvents::decode_log(&inner)?,
        block_hash,
        block_number,
        block_timestamp,
        transaction_hash,
        transaction_index,
        log_index,
        removed,
    })
}

fn log_position(log: &OMInteropLog) -> Option<(u64, u64)> {
    let block = log.block_number?;
    let index = log.log_index.unwrap_or(u64::MAX);
    Some((block, index))
}

fn should_emit(log_position: Option<(u64, u64)>, last_position: &mut Option<(u64, u64)>) -> bool {
    log_position.is_none_or(|position| {
        if last_position.is_some_and(|prev| position <= prev) {
            return false;
        }
        *last_position = Some(position);
        true
    })
}
