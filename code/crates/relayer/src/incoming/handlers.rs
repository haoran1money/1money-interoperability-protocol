use alloy_primitives::hex::ToHexExt;
use alloy_primitives::{Address, B256};
use alloy_signer_local::PrivateKeySigner;
use onemoney_interop::contract::OMInterop::{OMInteropReceived, OMInteropSent};
use onemoney_protocol::client::http::Client;
use onemoney_protocol::responses::TransactionResponse;
use onemoney_protocol::{PaymentPayload, TokenBridgeAndMintPayload};
use tracing::error;

use crate::incoming::error::Error as IncomingError;

pub struct Relayer1MoneyContext<'a> {
    client: &'a Client,
    relayer_address: Address,
    private_key_hex: String,
    chain_id: u64,
}

impl<'a> Relayer1MoneyContext<'a> {
    pub async fn new(
        client: &'a Client,
        relayer_signer: &PrivateKeySigner,
    ) -> Result<Self, IncomingError> {
        let relayer_address = relayer_signer.address();
        let private_key_hex = relayer_signer.to_bytes().encode_hex_with_prefix();
        let chain_id = client.fetch_chain_id_from_network().await?;

        Ok(Self {
            client,
            relayer_address,
            private_key_hex,
            chain_id,
        })
    }

    fn private_key(&self) -> &str {
        &self.private_key_hex
    }

    async fn validate_nonce(&self, sidechain_nonce: u64) -> Result<(), IncomingError> {
        let layer1_nonce = self
            .client
            .get_account_nonce(self.relayer_address)
            .await?
            .nonce;

        if layer1_nonce != sidechain_nonce {
            error!(
                %sidechain_nonce,
                %layer1_nonce,
                "Nonce mismatch"
            );
            return Err(IncomingError::NonceMismatch {
                sidechain: sidechain_nonce,
                layer1: layer1_nonce,
            });
        }

        Ok(())
    }

    pub async fn handle_om_interop_received(
        &self,
        OMInteropReceived {
            nonce: sidechain_nonce,
            to,
            amount,
            omToken: om_token,
            srcChainId: src_chain_id,
        }: OMInteropReceived,
        source_tx_hash: B256,
    ) -> Result<TransactionResponse, IncomingError> {
        self.validate_nonce(sidechain_nonce).await?;

        let recent_checkpoint = self.client.get_checkpoint_number().await?.number;

        let payload = TokenBridgeAndMintPayload {
            recent_checkpoint,
            chain_id: self.chain_id,
            nonce: sidechain_nonce,
            recipient: to,
            value: amount,
            token: om_token,
            source_chain_id: src_chain_id.into(),
            source_tx_hash: source_tx_hash.encode_hex_with_prefix(),
            bridge_metadata: None,
        };

        Ok(self
            .client
            .bridge_and_mint(payload, self.private_key())
            .await?)
    }

    pub async fn handle_om_interop_sent(
        &self,
        OMInteropSent {
            nonce: sidechain_nonce,
            from,
            refundAmount: refund_amount,
            omToken: om_token,
            dstChainId: _dst_chain_id,
        }: OMInteropSent,
    ) -> Result<TransactionResponse, IncomingError> {
        self.validate_nonce(sidechain_nonce).await?;

        let recent_checkpoint = self.client.get_checkpoint_number().await?.number;

        let payload = PaymentPayload {
            recent_checkpoint,
            chain_id: self.chain_id,
            nonce: sidechain_nonce,
            recipient: from,
            value: refund_amount,
            token: om_token,
        };

        Ok(self
            .client
            .send_payment(payload, self.private_key())
            .await?)
    }
}
