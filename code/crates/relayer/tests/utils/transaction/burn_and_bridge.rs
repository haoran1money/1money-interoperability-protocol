use core::ops::ControlFlow;

use alloy_primitives::hex::ToHexExt;
use alloy_primitives::{Address, U256};
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::Result;
use onemoney_protocol::responses::TransactionResponse;
use onemoney_protocol::{Client, TokenBurnAndBridgePayload};
use tracing::{info, warn};

use crate::utils::transaction::wait_for_transaction;
use crate::utils::{poll_with_timeout, MAX_DURATION, POLL_INTERVAL};

pub async fn burn_and_bridge(
    client_url: String,
    sender_wallet: PrivateKeySigner,
    destination_chain_id: u64,
    receiver: Address,
    token: Address,
    value: U256,
    escrow_fee: U256,
) -> Result<TransactionResponse> {
    let client = Client::custom(client_url.clone())?;
    let chain_id = client.fetch_chain_id_from_network().await?;
    let sender_addr = sender_wallet.address();

    poll_with_timeout("burn_and_bridge", POLL_INTERVAL, MAX_DURATION, {
        move || {
            let client_url_owned = client_url.clone();
            let sender_wallet_owned = sender_wallet.clone();
            let client = Client::custom(client_url_owned)
                .expect("failed to create 1Money client with url {client_url}");
            async move {
                let nonce = match client.get_account_nonce(sender_addr).await {
                    Ok(account_nonce) => account_nonce.nonce,
                    Err(err) => {
                        warn!(?err, "Failed to fetch operator context for token issuance");
                        return Ok(None);
                    }
                };

                let payload = TokenBurnAndBridgePayload {
                    chain_id,
                    nonce,
                    token,
                    value,
                    sender: sender_addr,
                    destination_chain_id,
                    destination_address: receiver.to_string(),
                    escrow_fee,
                    bridge_metadata: None,
                    bridge_param: None,
                };

                let response = match client
                    .burn_and_bridge(
                        payload,
                        &sender_wallet_owned.to_bytes().encode_hex_with_prefix(),
                    )
                    .await
                {
                    Ok(response) => response,
                    Err(err) => {
                        warn!(?err, "Burn and bridge submission failed");
                        return Ok(None);
                    }
                };
                let tx_hash = response.hash;
                match wait_for_transaction(&client, &tx_hash, "burn and bridge confirmation")
                    .await?
                {
                    ControlFlow::Break(()) => {
                        info!(%tx_hash, "Burn and bridge confirmed");
                        Ok(Some(response))
                    }
                    ControlFlow::Continue(()) => {
                        info!(%tx_hash, "Burn and bridge failed, retrying");
                        Ok(None)
                    }
                }
            }
        }
    })
    .await
}
