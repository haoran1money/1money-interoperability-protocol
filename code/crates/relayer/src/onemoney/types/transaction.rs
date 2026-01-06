//! The transaction types used in 1Money protocol.
//! Taken from https://github.com/1Money-Co/l1client/blob/e13451b01e82ee53058db104bfb244edaf56921b/crates/om-primitives/src/transaction/envelope.rs

use alloy_primitives::{Address, Bytes, B256, U256};
use onemoney_protocol::{Nonce, TxPayload};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedTransaction {
    pub result: CertifiedTransactionResult,
}

impl CertifiedTransaction {
    pub fn get_transaction_envelope(&self) -> &RawTransactionEnvelope {
        match &self.result {
            CertifiedTransactionResult {
                certificate:
                    CertificateEnvelope::V0(CertificateV0 {
                        tx: Transaction::UserTransaction(envelope),
                    }),
                ..
            } => &envelope.envelope,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedTransactionResult {
    pub certificate: CertificateEnvelope,
    pub tx_hash: B256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CertificateEnvelope {
    /// The first version of the certificate.
    V0(CertificateV0),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificateV0 {
    /// The transaction that submitted by users or the operator.
    tx: Transaction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Transaction {
    /// A transaction submitted by users
    // UserTransaction(Box<Recovered<TransactionSigned>>),
    UserTransaction(Box<TxEnvelope>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxEnvelope {
    /// The 1Money native transaction envelope that 1Money protocol
    /// supports.
    envelope: RawTransactionEnvelope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawTransactionEnvelope {
    /// Burn and bridge transaction variant.
    ///
    /// payload with the signature signed by the sender.
    TokenBurnAndBridge(TokenBurnAndBridge),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBurnAndBridge {
    pub payload: TokenBurnAndBridgePayload,
}

impl RawTransactionEnvelope {
    /// Returns the chain ID of the transaction.
    pub fn to_tx_payload(&self) -> TxPayload {
        match self {
            Self::TokenBurnAndBridge(token_burn_and_bridge) => {
                let payload = &token_burn_and_bridge.payload;
                TxPayload::TokenBurnAndBridge {
                    value: payload.value.to_string(),
                    sender: payload.sender,
                    destination_chain_id: payload.destination_chain_id,
                    destination_address: payload.destination_address.clone(),
                    escrow_fee: payload.escrow_fee.to_string(),
                    bridge_metadata: payload.bridge_metadata.clone(),
                    token: payload.token,
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenBurnAndBridgePayload {
    /// The chain id of the transaction.
    pub chain_id: u64,
    /// The nonce of the transaction.
    pub nonce: Nonce,
    /// This field is obsolete and will be ignored. The tokens will be burnt
    /// from the signer's wallet.
    pub sender: Address,
    /// The amount of tokens to burn for bridging.
    pub value: U256,
    /// The token address of the transaction.
    pub token: Address,
    /// The destination chain ID to bridge tokens to.
    pub destination_chain_id: u64,
    /// The destination address on the target chain.
    pub destination_address: String,
    /// The bridging fee necessary to escrow for transferring tokens to the
    /// destination chain.
    pub escrow_fee: U256,
    /// Optional bridge metadata for additional information.
    pub bridge_metadata: Option<String>,
    /// Optional bridge parameters as arbitrary bytes.
    pub bridge_param: Option<Bytes>,
}
