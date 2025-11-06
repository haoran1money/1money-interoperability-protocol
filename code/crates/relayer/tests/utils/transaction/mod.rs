use core::ops::ControlFlow;

use alloy_primitives::hex::ToHexExt;
use alloy_primitives::B256;
use color_eyre::eyre::{eyre, Result};
use onemoney_protocol::{Client, Error as OnemoneyError};
use tracing::{debug, warn};

use crate::utils::{poll_with_timeout, MAX_DURATION, POLL_INTERVAL};

pub mod burn_and_bridge;
pub mod types;

/// Polls a transaction hash until it confirms or tells us to resubmit.
///
/// `ControlFlow::Break` means the transaction is done; `ControlFlow::Continue` means to start over.
pub async fn wait_for_transaction(
    client: &Client,
    tx_hash: &B256,
    description: &str,
) -> Result<ControlFlow<(), ()>> {
    let tx_hash = tx_hash.encode_hex_with_prefix();
    poll_with_timeout(description, POLL_INTERVAL, MAX_DURATION, || async {
        match client.get_transaction_by_hash(&tx_hash).await {
            Ok(_) => Ok(Some(ControlFlow::Break(()))),
            Err(err) => {
                if is_transaction_missing(&err) {
                    debug!(%tx_hash, "Transaction not yet included");
                    Ok(None)
                } else if is_transaction_failure(&err) {
                    warn!(?err, %tx_hash, "Transaction failed, restarting submission");
                    Ok(Some(ControlFlow::Continue(())))
                } else {
                    warn!(?err, %tx_hash, "Unexpected error while polling transaction status");
                    Err(eyre!(err))
                }
            }
        }
    })
    .await
}

/// Returns `true` when the error says "transaction not found".
fn is_transaction_missing(err: &OnemoneyError) -> bool {
    match err {
        OnemoneyError::ResourceNotFound { resource_type, .. } => resource_type == "transaction",
        OnemoneyError::Api { error_code, .. } => {
            matches!(
                error_code.as_str(),
                "transaction_not_found" | "resource_transaction"
            )
        }
        _ => false,
    }
}

/// Returns `true` when the error says the transaction reverted.
fn is_transaction_failure(err: &OnemoneyError) -> bool {
    match err {
        OnemoneyError::BusinessLogic { operation, .. } => matches!(
            operation.as_str(),
            "transaction_failed" | "transaction_failure"
        ),
        OnemoneyError::Api { error_code, .. } => matches!(
            error_code.as_str(),
            "transaction_failed" | "business_transaction_failed" | "transaction_failure"
        ),
        _ => false,
    }
}
