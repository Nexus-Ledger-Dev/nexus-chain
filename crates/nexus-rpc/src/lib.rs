//! JSON-RPC API Module for NexusChain
//!
//! Provides Ethereum-compatible JSON-RPC API plus NexusChain extensions.

pub mod server;
pub mod methods;
pub mod types;
pub mod eth;
pub mod nexus;
pub mod mempool;

pub use server::*;
pub use methods::*;
pub use types::*;

/// Shared peer-count counter — the node layer increments/decrements this as
/// peers connect and disconnect; the RPC layer reads it for net_peerCount and
/// eth_syncing without depending on nexus-network directly.
pub type PeerCounter = std::sync::Arc<std::sync::atomic::AtomicUsize>;

use thiserror::Error;

/// Result type for RPC operations
pub type RpcResult<T> = Result<T, RpcError>;

/// RPC errors following JSON-RPC 2.0 spec
#[derive(Error, Debug, Clone)]
pub enum RpcError {
    #[error("Parse error")]
    ParseError,
    
    #[error("Invalid request")]
    InvalidRequest,
    
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    
    #[error("Invalid params: {0}")]
    InvalidParams(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    #[error("Transaction rejected: {0}")]
    TransactionRejected(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
}

impl RpcError {
    /// Get JSON-RPC error code
    pub fn code(&self) -> i32 {
        match self {
            RpcError::ParseError => -32700,
            RpcError::InvalidRequest => -32600,
            RpcError::MethodNotFound(_) => -32601,
            RpcError::InvalidParams(_) => -32602,
            RpcError::InternalError(_) => -32603,
            RpcError::TransactionRejected(_) => -32003,
            RpcError::NotFound(_) => -32004,
        }
    }
}
