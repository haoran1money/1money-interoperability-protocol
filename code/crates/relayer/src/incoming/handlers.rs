use core::sync::atomic::Ordering;

use alloy_primitives::hex::ToHexExt;
use alloy_primitives::{Address, B256};
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use onemoney_interop::contract::OMInterop::{OMInteropReceived, OMInteropSent};
use onemoney_interop::contract::TxHashMapping;
use onemoney_protocol::client::http::Client;
use onemoney_protocol::responses::TransactionResponse;
use onemoney_protocol::{PaymentPayload, TokenBridgeAndMintPayload};
use tracing::{debug, warn};

use crate::config::{Config, RelayerNonce};
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

    pub async fn should_process_nonce(&self, sidechain_nonce: u64) -> Result<bool, IncomingError> {
        let om_nonce = self
            .client
            .get_account_nonce(self.relayer_address)
            .await?
            .nonce;

        if om_nonce > sidechain_nonce {
            warn!(
                %sidechain_nonce,
                %om_nonce,
                "Layer 1 probably processed this nonce already: skip"
            );
            Ok(false)
        } else if om_nonce < sidechain_nonce {
            warn!(
                %sidechain_nonce,
                %om_nonce,
                "Layer 1 probably didn't process old nonces yet: could-wait-but-submit-to-mempool-anyway"
            );
            // TODO: Temporary workaround
            loop {
                let current_nonce = self
                    .client
                    .get_account_nonce(self.relayer_address)
                    .await?
                    .nonce;
                if current_nonce == sidechain_nonce {
                    debug!(
                        %sidechain_nonce,
                        %current_nonce,
                        "Nonce match: right-on-time"
                    );
                    break;
                }
                tokio::time::sleep(core::time::Duration::from_millis(10)).await;
            }
            Ok(true)
        } else {
            debug!(
                %sidechain_nonce,
                %om_nonce,
                "Nonce match: right-on-time"
            );
            Ok(true)
        }
    }

    pub async fn handle_om_interop_received(
        &self,
        config: &Config,
        relayer_nonce: RelayerNonce,
        OMInteropReceived {
            nonce: sidechain_nonce,
            to,
            amount,
            omToken: om_token,
            srcChainId: src_chain_id,
        }: OMInteropReceived,
        source_tx_hash: B256,
    ) -> Result<TransactionResponse, IncomingError> {
        let provider = ProviderBuilder::new()
            .wallet(config.relayer_private_key.clone())
            .connect_http(config.side_chain_http_url.clone());
        let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider);

        debug!(bridgeFromHash = %source_tx_hash, "Will register deposit transaction hash");

        mapping_contract
            .registerDeposit(source_tx_hash)
            .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
            .send()
            .await?
            .get_receipt()
            .await?;

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

        let tx_response = self
            .client
            .bridge_and_mint(payload, self.private_key())
            .await?;

        debug!(bridgeFromHash = %source_tx_hash, bridgeAndMintHash = %tx_response.hash, "Will link deposit transaction hashes");

        mapping_contract
            .linkDepositHashes(source_tx_hash, tx_response.hash)
            .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
            .send()
            .await?
            .get_receipt()
            .await?;

        Ok(tx_response)
    }

    pub async fn handle_om_interop_sent(
        &self,
        config: &Config,
        relayer_nonce: RelayerNonce,
        OMInteropSent {
            nonce: sidechain_nonce,
            from,
            refundAmount: refund_amount,
            omToken: om_token,
            dstChainId: _dst_chain_id,
            sourceHash: source_hash,
        }: OMInteropSent,
    ) -> Result<TransactionResponse, IncomingError> {
        let provider = ProviderBuilder::new()
            .wallet(config.relayer_private_key.clone())
            .connect_http(config.side_chain_http_url.clone());
        let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider);

        let recent_checkpoint = self.client.get_checkpoint_number().await?.number;

        let payload = PaymentPayload {
            recent_checkpoint,
            chain_id: self.chain_id,
            nonce: sidechain_nonce,
            recipient: from,
            value: refund_amount,
            token: om_token,
        };

        let tx_response = self
            .client
            .send_payment(payload, self.private_key())
            .await?;

        debug!(burnAndBridgeHas = %source_hash, refundHash = %tx_response.hash, "Will link refund transaction hash");

        mapping_contract
            .linkRefundHashes(source_hash, tx_response.hash)
            .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
            .send()
            .await
            .map(Ok)
            .or_else(|e| {
                e.try_decode_into_interface_error::<TxHashMapping::TxHashMappingErrors>()
                    .map(Err)
            })?
            .map_err(IncomingError::MappingContractReverted)?
            .get_receipt()
            .await?;

        Ok(tx_response)
    }
}
