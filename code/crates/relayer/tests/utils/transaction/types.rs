use alloy_primitives::Address;
use alloy_rlp::{RlpDecodable, RlpEncodable};
use onemoney_protocol::{Hash, Signature};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct TokenIssuePayload {
    pub chain_id: u64,
    pub nonce: u64,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub master_authority: Address,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenIssueRequest {
    #[serde(flatten)]
    pub payload: TokenIssuePayload,
    pub signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenIssueResponse {
    pub hash: Hash,
    pub token: Address,
}
