use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Sidechain(#[from] crate::sidechain::error::Error),
    #[error(transparent)]
    Onemoney(#[from] crate::onemoney::error::Error),
    #[error(transparent)]
    Poa(#[from] crate::poa::error::Error),
}
