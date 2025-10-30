#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transport error: {0}")]
    Transport(#[from] alloy_transport::TransportError),
    #[error("event decode error: {0}")]
    Decode(#[from] alloy_sol_types::Error),
}
