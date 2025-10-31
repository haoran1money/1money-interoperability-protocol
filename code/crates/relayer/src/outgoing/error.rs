use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("1Money error: {0}")]
    Onemoney(#[from] onemoney_protocol::error::Error),
    #[error("Failed to create alloy address: {0}")]
    CreateAddress(#[from] alloy_primitives::hex::FromHexError),
    #[error("Failed to parse Alloy primitive: {0}")]
    ParseInt(#[from] alloy_primitives::ruint::ParseError),
    #[error("Failed to convert integer: {0}")]
    ConvertInt(#[from] core::num::TryFromIntError),
    #[error("Contract call failed: {0}")]
    ContractCall(#[from] alloy_contract::Error),
    #[error("Pending transaction failed: {0}")]
    PendingTransaction(#[from] alloy_provider::PendingTransactionError),
    #[error(transparent)]
    Sidechain(#[from] crate::onemoney::error::Error),
    #[error("Generic error: {0}")]
    Generic(String),
}
