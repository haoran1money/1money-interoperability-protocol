use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    EventStream(#[from] onemoney_interop::error::Error),
    #[error(transparent)]
    Onemoney(#[from] onemoney_protocol::Error),
}
