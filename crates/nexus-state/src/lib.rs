//! State Storage Module for NexusChain
//!
//! Provides state management with Merkle Patricia Trie support.

pub mod trie;
pub mod storage;
pub mod checkpoint;

pub use trie::*;
pub use storage::*;
pub use checkpoint::*;

use thiserror::Error;

/// Result type for state operations
pub type StateResult<T> = Result<T, StateError>;

/// State errors
#[derive(Error, Debug)]
pub enum StateError {
    #[error("Key not found: {0}")]
    NotFound(String),
    
    #[error("Invalid proof")]
    InvalidProof,
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Checkpoint error: {0}")]
    Checkpoint(String),
}
