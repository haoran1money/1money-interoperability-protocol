use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    RelayerNonce(#[from] alloy_transport::RpcError<alloy_transport::TransportErrorKind>),
}
