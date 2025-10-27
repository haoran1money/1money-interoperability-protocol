use alloy_primitives::{Address, B256, U256};
use alloy_signer::k256::ecdsa::VerifyingKey;
use serde::{Deserialize, Deserializer};
use validator_manager::ValidatorManager::{Secp256k1Key, ValidatorInfo};

use crate::onemoney::error::Error as OnemoneyError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Validator {
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub consensus_public_key: VerifyingKey,
    pub address: Address,
    pub peer_id: String,
    pub archive: bool,
}

impl core::hash::Hash for Validator {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.consensus_public_key.to_encoded_point(true).hash(state);
        self.address.hash(state);
        self.peer_id.hash(state);
        self.archive.hash(state);
    }
}

fn deserialize_verifying_key<'de, D>(deserializer: D) -> Result<VerifyingKey, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let bytes = hex::decode(s.trim_start_matches("0x")).map_err(serde::de::Error::custom)?;
    VerifyingKey::from_sec1_bytes(&bytes).map_err(serde::de::Error::custom)
}

impl TryFrom<Validator> for ValidatorInfo {
    type Error = OnemoneyError;

    fn try_from(v: Validator) -> Result<Self, Self::Error> {
        let Validator {
            consensus_public_key,
            address,
            ..
        } = v;
        let pubkey = consensus_public_key.to_encoded_point(false);
        let x = U256::from_be_slice(
            pubkey
                .x()
                .ok_or(OnemoneyError::InvalidValidatorKey { address })?,
        );
        let y = U256::from_be_slice(
            pubkey
                .y()
                .ok_or(OnemoneyError::InvalidValidatorKey { address })?,
        );
        Ok(Self {
            validatorKey: Secp256k1Key { x, y },
            power: 100, // Default power, update as needed
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ValidatorSet {
    pub members: Vec<Validator>,
}

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
    pub operator_public_key: String,
    pub operator_address: String,
    pub validator_set: ValidatorSet,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct EpochId {
    pub epoch_id: u64,
}

#[cfg(test)]
mod tests {
    use alloy_primitives::address;
    use alloy_signer::k256::ecdsa::SigningKey;
    use alloy_signer::k256::Scalar;
    use serde_json::json;

    use super::*;

    #[test]
    fn validator_deserialization_round_trip() {
        let signing_key =
            SigningKey::from_bytes(&Scalar::from(42u64).to_bytes()).expect("valid key");
        let verifying_key = *signing_key.verifying_key();
        let key_hex = format!(
            "0x{}",
            hex::encode(verifying_key.to_encoded_point(true).as_bytes())
        );
        let value = json!({
            "consensus_public_key": key_hex,
            "address": format!(
                "{:#x}",
                address!("0x0000000000000000000000000000000000000042")
            ),
            "peer_id": "peer-42",
            "archive": false
        });

        let validator: Validator =
            serde_json::from_value(value).expect("validator should deserialize");
        assert_eq!(validator.consensus_public_key, verifying_key);
        assert_eq!(
            validator.address,
            address!("0x0000000000000000000000000000000000000042")
        );
        assert_eq!(validator.peer_id, "peer-42");
        assert!(!validator.archive);
    }

    #[test]
    fn validator_deserialization_rejects_bad_hex() {
        let value = json!({
            "consensus_public_key": "0xzz",
            "address": format!(
                "{:#x}",
                address!("0x0000000000000000000000000000000000000001")
            ),
            "peer_id": "peer",
            "archive": false
        });

        let err = serde_json::from_value::<Validator>(value).expect_err("should fail to decode");
        let err_msg = err.to_string();
        assert!(
            err_msg.to_lowercase().contains("invalid character"),
            "unexpected error message: {err_msg}"
        );
    }
}
