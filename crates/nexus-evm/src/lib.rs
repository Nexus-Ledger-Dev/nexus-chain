//! EVM Execution Module for NexusChain
//! 
//! Provides full Ethereum Virtual Machine compatibility with extensions for:
//! - ZKP verification precompiles
//! - ISO 20022 message handling precompiles
//! - DAG-aware state management

pub mod executor;
pub mod state;
pub mod precompiles;
pub mod gas;
pub mod receipt;

pub use executor::*;
pub use state::*;
pub use precompiles::*;
pub use gas::*;
pub use receipt::*;

use thiserror::Error;

/// Result type for EVM operations
pub type EvmResult<T> = Result<T, EvmError>;

/// EVM execution errors
#[derive(Error, Debug, Clone)]
pub enum EvmError {
    #[error("Execution reverted: {0}")]
    Revert(String),
    
    #[error("Out of gas: used {used}, limit {limit}")]
    OutOfGas { used: u64, limit: u64 },
    
    #[error("Invalid opcode: 0x{0:02x}")]
    InvalidOpcode(u8),
    
    #[error("Stack underflow")]
    StackUnderflow,
    
    #[error("Stack overflow")]
    StackOverflow,
    
    #[error("Invalid jump destination: {0}")]
    InvalidJump(u64),
    
    #[error("State error: {0}")]
    StateError(String),
    
    #[error("Precompile error: {0}")]
    PrecompileError(String),
    
    #[error("Contract creation failed: {0}")]
    CreateFailed(String),
    
    #[error("Nonce mismatch: expected {expected}, got {got}")]
    NonceMismatch { expected: u64, got: u64 },
    
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: String, available: String },
    
    #[error("Invalid signature")]
    InvalidSignature,
}
