use alloy_primitives::{Address, B256};
use alloy_signer::k256::ecdsa::VerifyingKey;
use serde::Deserialize;

use crate::onemoney::types::utils::deserialize_verifying_key;
use crate::onemoney::types::validator::ValidatorSet;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Epoch {
    pub epoch_id: u64,
    pub hash: B256,
    pub validator_set: ValidatorSet,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawEpoch {
    pub epoch_id: u64,
    pub hash: B256,
    pub certificate: Certificate,
}

impl From<RawEpoch> for Epoch {
    fn from(raw: RawEpoch) -> Self {
        Self {
            epoch_id: raw.epoch_id,
            hash: raw.hash,
            validator_set: raw.certificate.genesis.proposal.message.validator_set,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Certificate {
    #[serde(rename = "Genesis")]
    pub genesis: Genesis,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Genesis {
    pub proposal: Proposal,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub message: Message,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Message {
    pub epoch: EpochId,
    pub chain: u64,
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub operator_public_key: VerifyingKey,
    pub operator_address: Address,
    pub validator_set: ValidatorSet,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct EpochId {
    pub epoch_id: u64,
}
