use core::ops::ControlFlow;
use core::str::FromStr;

use alloy_primitives::{keccak256, Address, U256};
use alloy_rlp::Encodable;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::Result;
use onemoney_protocol::crypto::signing::sign_hash;
use onemoney_protocol::responses::TransactionResponse;
use onemoney_protocol::{
    Authority, AuthorityAction, Client, TokenAuthorityPayload, TokenMintPayload,
};
use tracing::{debug, info, warn};

use super::account::fetch_account_context;
use super::transaction::types::{TokenIssuePayload, TokenIssueRequest, TokenIssueResponse};
use super::{poll_with_timeout, MAX_DURATION, POLL_INTERVAL};
use crate::utils::transaction::wait_for_transaction;

pub const OPERATOR_PRIVATE_KEY: &str =
    "0x76700ba1cb72480053d43b6202a16e9acbfb318b0321cfac4e55d38747bf9057";

pub struct OperationClient<'a> {
    pub client: &'a Client,
    pub private_key: &'a str,
}

impl<'a> OperationClient<'a> {
    pub fn new(client: &'a Client, private_key: &'a str) -> Self {
        Self {
            client,
            private_key,
        }
    }

    /// Issues a new token and waits for the issue transaction to confirm.
    pub async fn issue_new_token(&self, symbol: &str, name: &str, decimals: u8) -> Result<Address> {
        let operator_address = PrivateKeySigner::from_str(self.private_key)?.address();
        let chain_id = self.client.fetch_chain_id_from_network().await?;
        let symbol_owned = symbol.to_string();
        let name_owned = name.to_string();
        let client = self.client;
        let private_key = self.private_key;

        poll_with_timeout("token issuance", POLL_INTERVAL, MAX_DURATION, {
            let symbol = symbol_owned.clone();
            let name = name_owned.clone();
            move || {
                let symbol = symbol.clone();
                let name = name.clone();
                async move {
                    let (recent_checkpoint, nonce) =
                        match fetch_account_context(client, operator_address).await {
                            Ok(context) => context,
                            Err(err) => {
                                warn!(?err, "Failed to fetch operator context for token issuance");
                                return Ok(None);
                            }
                        };

                    let payload = TokenIssuePayload {
                        recent_checkpoint,
                        chain_id,
                        nonce,
                        symbol,
                        name,
                        decimals,
                        master_authority: operator_address,
                        is_private: false,
                    };

                    let signature = {
                        let mut encoded = Vec::new();
                        payload.encode(&mut encoded);
                        let signature_hash = keccak256(encoded);
                        sign_hash(&signature_hash, private_key)?
                    };

                    let request_body = TokenIssueRequest { payload, signature };
                    debug!("Submitting token issuance request");

                    let response: TokenIssueResponse =
                        match client.post("v1/tokens/issue", &request_body).await {
                            Ok(response) => response,
                            Err(err) => {
                                warn!(?err, "Token issuance submission failed");
                                return Ok(None);
                            }
                        };

                    debug!(
                        ?response,
                        "Token issuance transaction submitted, waiting for confirmation"
                    );

                    let tx_hash = response.hash.hash;
                    match wait_for_transaction(client, &tx_hash, "token issuance confirmation")
                        .await?
                    {
                        ControlFlow::Break(()) => {
                            info!(%tx_hash, "Token issuance confirmed");
                            Ok(Some(response.token))
                        }
                        ControlFlow::Continue(()) => {
                            info!(%tx_hash, "Token issuance failed, retrying");
                            Ok(None)
                        }
                    }
                }
            }
        })
        .await
    }

    pub async fn mint_token(
        &self,
        recipient: Address,
        value: U256,
        token: Address,
    ) -> Result<TransactionResponse> {
        let operator_address = PrivateKeySigner::from_str(self.private_key)?.address();
        let chain_id = self.client.fetch_chain_id_from_network().await?;
        let client = self.client;
        let private_key = self.private_key;

        poll_with_timeout("token mint_token", POLL_INTERVAL, MAX_DURATION, {
            move || async move {
                let (recent_checkpoint, nonce) =
                    match fetch_account_context(client, operator_address).await {
                        Ok(context) => context,
                        Err(err) => {
                            warn!(?err, "Failed to fetch operator context for token issuance");
                            return Ok(None);
                        }
                    };

                let payload = TokenMintPayload {
                    recent_checkpoint,
                    chain_id,
                    nonce,
                    recipient,
                    value,
                    token,
                };

                debug!("Submitting token mint request");

                let response = match client.mint_token(payload, private_key).await {
                    Ok(response) => response,
                    Err(err) => {
                        warn!(?err, "Token mint submission failed");
                        return Ok(None);
                    }
                };

                let tx_hash = response.hash;
                match wait_for_transaction(client, &tx_hash, "token mint confirmation").await? {
                    ControlFlow::Break(()) => {
                        info!(%tx_hash, "Token mint confirmed");
                        Ok(Some(response))
                    }
                    ControlFlow::Continue(()) => {
                        info!(%tx_hash, "Token mint failed, retrying");
                        Ok(None)
                    }
                }
            }
        })
        .await
    }

    /// Grants the requested authority and waits for the grant transaction to confirm.
    pub async fn grant_authority(
        &self,
        authority_type: Authority,
        authority_address: Address,
        token: Address,
        value: U256,
    ) -> Result<TransactionResponse> {
        let operator_address = PrivateKeySigner::from_str(self.private_key)?.address();
        let chain_id = self.client.fetch_chain_id_from_network().await?;
        let client = self.client;
        let private_key = self.private_key;

        poll_with_timeout(
            "authority grant",
            POLL_INTERVAL,
            MAX_DURATION,
            move || async move {
                let (recent_checkpoint, nonce) =
                    match fetch_account_context(client, operator_address).await {
                        Ok(context) => context,
                        Err(err) => {
                            warn!(
                                ?err,
                                ?authority_type,
                                "Failed to fetch operator context for authority grant"
                            );
                            return Ok(None);
                        }
                    };

                let payload = TokenAuthorityPayload {
                    recent_checkpoint,
                    chain_id,
                    nonce,
                    token,
                    action: AuthorityAction::Grant,
                    authority_type,
                    authority_address,
                    value,
                };

                let response = match client.grant_authority(payload, private_key).await {
                    Ok(response) => response,
                    Err(err) => {
                        warn!(?err, ?authority_type, "Authority grant submission failed");
                        return Ok(None);
                    }
                };

                let tx_hash = response.hash;
                match wait_for_transaction(client, &tx_hash, "authority grant confirmation").await?
                {
                    ControlFlow::Break(()) => {
                        info!(%tx_hash, ?authority_type, "Authority grant confirmed");
                        Ok(Some(response))
                    }
                    ControlFlow::Continue(()) => {
                        warn!(%tx_hash, ?authority_type, "Authority grant failed, retrying");
                        Ok(None)
                    }
                }
            },
        )
        .await
    }
}
