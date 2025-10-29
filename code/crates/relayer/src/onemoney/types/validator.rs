use alloy_primitives::{Address, U256};
use alloy_signer::k256::ecdsa::VerifyingKey;
use serde::Deserialize;
use validator_manager::ValidatorManager::{Secp256k1Key, ValidatorInfo};

use crate::onemoney::error::Error as OnemoneyError;
use crate::onemoney::types::utils::deserialize_verifying_key;

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
