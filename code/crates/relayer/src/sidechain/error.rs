#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to process new validator set: {0}")]
    ProcessValidatorSet(#[from] alloy_contract::Error),
    #[error("Failed to get transaction receipt: {0}")]
    PendingTransactionReceipt(#[from] alloy_provider::PendingTransactionError),
    #[error("Contract reverted: {0:?}")]
    ValidatorManagerContractReverted(validator_manager::ValidatorManager::ValidatorManagerErrors),
}
