use alloy_primitives::Address;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to construct URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Validator {address:?} has an invalid consensus public key")]
    InvalidValidatorKey { address: Address },
    #[error("Failed to query 1Money: {0}")]
    FailedQuery(#[from] onemoney_protocol::Error),
    #[error("Pending transaction failed: {0}")]
    PendingTransaction(#[from] alloy_provider::PendingTransactionError),
    #[error("Contract call failed: {0}")]
    ContractCall(#[from] alloy_contract::Error),
    #[error("Failed: {0}")]
    Generic(String),
}
