use alloy_primitives::{Address, B256};
use alloy_signer::k256::ecdsa::VerifyingKey;
use serde::Deserialize;

use crate::onemoney::types::utils::deserialize_verifying_key;
use crate::onemoney::types::validator::ValidatorSet;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Epoch {
    pub epoch_id: u64,
    pub certificate_hash: B256,
    pub validator_set: ValidatorSet,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawEpoch {
    pub epoch_id: u64,
    pub certificate_hash: B256,
    pub certificate: Certificate,
}

impl From<RawEpoch> for Epoch {
    fn from(raw: RawEpoch) -> Self {
        let message = match raw.certificate {
            Certificate::Genesis { proposal } => proposal.message,
            Certificate::Epoch { proposal } => proposal.message,
        };
        Self {
            epoch_id: raw.epoch_id,
            certificate_hash: raw.certificate_hash,
            validator_set: message.validator_set,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Certificate {
    Genesis { proposal: GenesisProposal },
    Epoch { proposal: GovernanceProposal },
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct GenesisProposal {
    pub message: Message,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct GovernanceProposal {
    pub message: Message,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Message {
    pub epoch: EpochId,
    pub chain: u64,
    pub special_accounts: SpecialAccounts,
    pub validator_set: ValidatorSet,
}
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct SpecialAccounts {
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub operator_public_key: VerifyingKey,
    pub operator_address: Address,
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub escrow_account_public_key: VerifyingKey,
    pub escrow_account_address: Address,
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub pricing_authority_public_key: VerifyingKey,
    pub pricing_authority_address: Address,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct EpochId {
    pub epoch_id: u64,
}
