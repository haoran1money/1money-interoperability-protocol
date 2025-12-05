use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("1Money error: {0}")]
    Onemoney(#[from] onemoney_protocol::error::Error),
    #[error("Failed to create alloy address: {0}")]
    CreateAddress(#[from] alloy_primitives::hex::FromHexError),
    #[error("Failed to parse Alloy primitive: {0}")]
    ParseInt(#[from] alloy_primitives::ruint::ParseError),
    #[error("Failed to convert integer: {0}")]
    ConvertInt(#[from] core::num::TryFromIntError),
    #[error("Contract call failed: {0}")]
    ContractCall(#[from] alloy_contract::Error),
    #[error("Pending transaction failed: {0}")]
    PendingTransaction(#[from] alloy_provider::PendingTransactionError),
    #[error("Alloy RPC Transport error: {0}")]
    ContractRpcTransport(#[from] alloy_transport::RpcError<alloy_transport::TransportErrorKind>),
    #[error(transparent)]
    Sidechain(#[from] crate::onemoney::error::Error),
    #[error("Contract reverted: {0:?}")]
    ContractReverted(onemoney_interop::contract::OMInterop::OMInteropErrors),
    #[error("Contract reverted: {0:?}")]
    MappingContractReverted(onemoney_interop::contract::TxHashMapping::TxHashMappingErrors),
    #[error("Missing checkpoint number in transaction")]
    MissingCheckpointNumber,
    #[error("Generic error: {0}")]
    Generic(String),
}
