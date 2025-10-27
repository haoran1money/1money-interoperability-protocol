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
}
