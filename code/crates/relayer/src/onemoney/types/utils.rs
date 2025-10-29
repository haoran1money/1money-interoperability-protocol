use alloy_signer::k256::ecdsa::VerifyingKey;
use serde::{Deserialize, Deserializer};

pub fn deserialize_verifying_key<'de, D>(deserializer: D) -> Result<VerifyingKey, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let bytes = hex::decode(s.trim_start_matches("0x")).map_err(serde::de::Error::custom)?;
    VerifyingKey::from_sec1_bytes(&bytes).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::address;
    use alloy_signer::k256::ecdsa::SigningKey;
    use alloy_signer::k256::Scalar;
    use alloy_signer::utils::public_key_to_address;
    use serde_json::json;

    use crate::onemoney::types::validator::Validator;

    #[test]
    fn validator_deserialization_round_trip() {
        let signing_key =
            SigningKey::from_bytes(&Scalar::from(42u64).to_bytes()).expect("valid key");
        let verifying_key = *signing_key.verifying_key();
        let key_hex = format!(
            "0x{}",
            hex::encode(verifying_key.to_encoded_point(true).as_bytes())
        );
        let address = public_key_to_address(&verifying_key);
        let value = json!({
            "consensus_public_key": key_hex,
            "address": address,
            "peer_id": "peer-42",
            "archive": false
        });

        let validator: Validator =
            serde_json::from_value(value).expect("validator should deserialize");
        assert_eq!(validator.consensus_public_key, verifying_key);
        assert_eq!(validator.address, address);
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
