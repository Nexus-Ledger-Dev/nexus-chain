//! Error types for NexusChain

use thiserror::Error;

/// Main error type for NexusChain operations
#[derive(Error, Debug)]
pub enum NexusError {
    #[error("Cryptographic error: {0}")]
    Crypto(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Consensus error: {0}")]
    Consensus(String),
    
    #[error("State error: {0}")]
    State(String),
    
    #[error("EVM execution error: {0}")]
    Evm(String),
    
    #[error("ZKP verification error: {0}")]
    Zkp(String),
    
    #[error("ISO compliance error: {0}")]
    Iso(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Insufficient funds: required {required}, available {available}")]
    InsufficientFunds { required: String, available: String },
    
    #[error("Nonce mismatch: expected {expected}, got {got}")]
    NonceMismatch { expected: u64, got: u64 },
    
    #[error("Gas limit exceeded: limit {limit}, used {used}")]
    GasLimitExceeded { limit: u64, used: u64 },
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("DAG error: {0}")]
    Dag(String),
    
    #[error("Vertex not found: {0}")]
    VertexNotFound(String),
    
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
    
    #[error("Invalid proof")]
    InvalidProof,
    
    #[error("Duplicate nullifier")]
    DuplicateNullifier,
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<std::io::Error> for NexusError {
    fn from(err: std::io::Error) -> Self {
        NexusError::Storage(err.to_string())
    }
}

impl From<serde_json::Error> for NexusError {
    fn from(err: serde_json::Error) -> Self {
        NexusError::Serialization(err.to_string())
    }
}

impl From<bincode::Error> for NexusError {
    fn from(err: bincode::Error) -> Self {
        NexusError::Serialization(err.to_string())
    }
}

impl From<hex::FromHexError> for NexusError {
    fn from(err: hex::FromHexError) -> Self {
        NexusError::Validation(format!("Invalid hex: {}", err))
    }
}

/// Result type alias for NexusChain operations
pub type Result<T> = std::result::Result<T, NexusError>;

/// Conversion trait for mapping errors
pub trait IntoNexusError<T> {
    fn into_nexus_error(self, context: &str) -> Result<T>;
}

impl<T, E: std::fmt::Display> IntoNexusError<T> for std::result::Result<T, E> {
    fn into_nexus_error(self, context: &str) -> Result<T> {
        self.map_err(|e| NexusError::Unknown(format!("{}: {}", context, e)))
    }
}
