use alloy_primitives::{Address, U256};
use color_eyre::eyre::{eyre, Result};
use onemoney_protocol::Client;
use tracing::debug;

use crate::utils::{poll_with_timeout, MAX_DURATION, POLL_INTERVAL};

/// Fetches the latest checkpoint number and account nonce for `address`.
pub async fn fetch_account_context(client: &Client, address: Address) -> Result<(u64, u64)> {
    let recent_checkpoint = client.get_checkpoint_number().await?.number;
    let nonce = client.get_account_nonce(address).await?.nonce;
    Ok((recent_checkpoint, nonce))
}

/// Reads the balance for `token` and `address`, defaulting to zero if it is missing.
pub async fn fetch_balance(client: &Client, address: Address, token: Address) -> Result<U256> {
    match client.get_associated_token_account(address, token).await {
        Ok(account) => U256::from_str_radix(&account.balance, 10)
            .map_err(|err| eyre!("invalid balance string {}: {err}", account.balance)),
        Err(err) => {
            debug!(
                ?address,
                ?token,
                error = %err,
                "Token account lookup failed, assuming zero balance"
            );
            Ok(U256::ZERO)
        }
    }
}

/// Waits for the balance to increase above `initial_balance`.
pub async fn wait_for_balance_increase(
    client: &Client,
    address: Address,
    token: Address,
    initial_balance: U256,
) -> Result<U256> {
    poll_with_timeout(
        "balance increase",
        POLL_INTERVAL,
        MAX_DURATION,
        || async move {
            let balance = fetch_balance(client, address, token).await?;
            if balance > initial_balance {
                return Ok(Some(balance));
            }

            debug!(
                %initial_balance,
                %balance,
                "Balance has not increased yet, retrying"
            );

            Ok(None)
        },
    )
    .await
}
