pub mod error;
use core::sync::atomic::Ordering;
use std::collections::HashSet;

use alloy_provider::ProviderBuilder;
use tracing::{debug, info};
use validator_manager::ValidatorManager::{self, Secp256k1Key, ValidatorInfo};
use validator_manager::CONTRACT_ADDRESS;

use crate::config::{Config, RelayerNonce};
use crate::sidechain::error::Error as SideChainError;

pub async fn process_new_validator_set(
    config: &Config,
    relayer_nonce: RelayerNonce,
    new_validators: Vec<ValidatorInfo>,
) -> Result<(), SideChainError> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_node_url.clone());
    let contract = ValidatorManager::new(CONTRACT_ADDRESS, provider.clone());

    // Fetch current validator set from contract
    let old_validators = contract.getValidators().call().await?;
    debug!(?old_validators, "Old validator set");

    let (add_validators, remove_validator_keys) =
        compute_validator_diffs(&old_validators, &new_validators);

    debug!(
        ?add_validators,
        ?remove_validator_keys,
        "Validator set diff"
    );

    if add_validators.is_empty() && remove_validator_keys.is_empty() {
        info!("Validator set already up to date; skipping update");
        return Ok(());
    }

    // Send transaction to update validator set
    let tx_receipt = contract
        .addAndRemove(add_validators, remove_validator_keys)
        .nonce(relayer_nonce.fetch_add(1, Ordering::SeqCst))
        .send()
        .await?
        .get_receipt()
        .await?;

    info!(?tx_receipt, "Tx receipt for validator set update");

    // Query new validator set
    let new_validators = contract.getValidators().call().await?;
    debug!(?new_validators, "New validator set");

    Ok(())
}

fn compute_validator_diffs(
    old_validators: &[ValidatorInfo],
    new_validators: &[ValidatorInfo],
) -> (Vec<ValidatorInfo>, Vec<Secp256k1Key>) {
    let old_validators = old_validators
        .iter()
        .cloned()
        .collect::<HashSet<ValidatorInfo>>();
    let new_validators = new_validators
        .iter()
        .cloned()
        .collect::<HashSet<ValidatorInfo>>();

    let mut add_validators: Vec<_> = new_validators
        .difference(&old_validators)
        .cloned()
        .collect();
    add_validators.sort();

    let mut remove_validator_keys: Vec<_> = old_validators
        .difference(&new_validators)
        .map(|v| v.validatorKey.clone())
        .collect();
    remove_validator_keys.sort();

    (add_validators, remove_validator_keys)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::U256;
    use alloy_signer::k256::ecdsa::SigningKey;
    use alloy_signer::k256::Scalar;

    use super::*;

    fn make_validator_info(index: u64) -> ValidatorInfo {
        let signing_key =
            SigningKey::from_bytes(&Scalar::from(index + 1).to_bytes()).expect("valid key");
        let verifying_key = signing_key.verifying_key();
        let point = verifying_key.to_encoded_point(false);
        let x = U256::from_be_slice(point.x().expect("x coordinate"));
        let y = U256::from_be_slice(point.y().expect("y coordinate"));

        ValidatorInfo {
            validatorKey: Secp256k1Key { x, y },
            power: index + 1,
        }
    }

    #[test]
    fn compute_validator_diffs_no_changes() {
        let validator = make_validator_info(0);
        let (add, remove) = compute_validator_diffs(
            core::slice::from_ref(&validator),
            core::slice::from_ref(&validator),
        );
        assert!(add.is_empty());
        assert!(remove.is_empty());
    }

    #[test]
    fn compute_validator_diffs_add_and_remove() {
        let v1 = make_validator_info(1);
        let v2 = make_validator_info(2);
        let v3 = make_validator_info(3);

        let old = [v1.clone(), v2.clone()];
        let new = [v2, v3.clone()];
        let (add, remove) = compute_validator_diffs(&old, &new);

        assert_eq!(add, vec![v3]);
        assert_eq!(remove, vec![v1.validatorKey]);
    }
}
