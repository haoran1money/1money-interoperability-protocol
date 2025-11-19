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
    #[error(transparent)]
    Contract(#[from] alloy_contract::Error),
    #[error("Pending transaction failed: {0}")]
    PendingTransaction(#[from] alloy_provider::PendingTransactionError),
    #[error("Contract reverted: {0:?}")]
    MappingContractReverted(onemoney_interop::contract::TxHashMapping::TxHashMappingErrors),
    #[error(transparent)]
    RpcTransport(#[from] alloy_transport::RpcError<alloy_transport::TransportErrorKind>),
    #[error("Contract reverted: {0:?}")]
    ContractReverted(onemoney_interop::contract::OMInterop::OMInteropErrors),
    #[error("Generic error: {0}")]
    Generic(String),
}
