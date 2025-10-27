use alloy_primitives::Address;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Onemoney(#[from] crate::onemoney::error::Error),
    #[error(transparent)]
    Sidechain(#[from] crate::sidechain::error::Error),
    #[error("Validator {address:?} has an invalid consensus public key")]
    InvalidValidatorKey { address: Address },
}
