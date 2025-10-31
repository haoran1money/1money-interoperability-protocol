use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    EventStream(#[from] onemoney_interop::error::Error),
    #[error(transparent)]
    Onemoney(#[from] onemoney_protocol::Error),
    #[error("Missing block number in event")]
    MissingBlockNumber,
    #[error("Missing log index in event")]
    MissingLogIndex,
    #[error("Missing transaction hash in event")]
    MissingTransactionHash,
    #[error("Relayer account nonce mismatch: sidechain={sidechain}, layer1={layer1}")]
    NonceMismatch { sidechain: u64, layer1: u64 },
}
