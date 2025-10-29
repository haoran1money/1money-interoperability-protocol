use alloy_signer::k256::ecdsa::{SigningKey, VerifyingKey};

pub fn consensus_key() -> VerifyingKey {
    let signing_key = SigningKey::from_bytes(&alloy_signer::k256::Scalar::from(7u64).to_bytes())
        .expect("valid key");
    *signing_key.verifying_key()
}
