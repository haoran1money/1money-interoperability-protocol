use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] crate::config::error::Error),
    #[error(transparent)]
    Sidechain(#[from] crate::sidechain::error::Error),
    #[error(transparent)]
    Onemoney(#[from] crate::onemoney::error::Error),
    #[error(transparent)]
    Poa(#[from] crate::poa::error::Error),
    #[error(transparent)]
    Incoming(#[from] crate::incoming::error::Error),
    #[error(transparent)]
    Outgoing(#[from] crate::outgoing::error::Error),
}
