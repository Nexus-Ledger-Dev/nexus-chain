//! Consensus Module for NexusChain
//! 
//! Implements a hybrid consensus mechanism combining:
//! - DAG-based probabilistic finality (like IOTA/Avalanche)
//! - BFT finality gadget for deterministic finality
//! - Proof of Stake validator selection

pub mod validator;
pub mod pos;
pub mod bft;
pub mod proposer;
pub mod iso_logger;

#[cfg(feature = "hybrid_consensus")]
pub mod hybrid_consensus;

#[cfg(feature = "hybrid_consensus")]
pub use hybrid_consensus::*;

pub use validator::*;
pub use pos::*;
pub use bft::*;
pub use proposer::*;

use thiserror::Error;

/// Result type for consensus operations
pub type ConsensusResult<T> = Result<T, ConsensusError>;

/// Consensus errors
#[derive(Error, Debug, Clone)]
pub enum ConsensusError {
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Unknown validator: {0}")]
    UnknownValidator(String),
    
    #[error("Insufficient stake: required {required}, available {available}")]
    InsufficientStake { required: u64, available: u64 },
    
    #[error("Invalid proposal: {0}")]
    InvalidProposal(String),
    
    #[error("Duplicate vote")]
    DuplicateVote,
    
    #[error("Invalid vote: {0}")]
    InvalidVote(String),
    
    #[error("Consensus not reached")]
    NotReached,
    
    #[error("Slashing condition: {0}")]
    SlashingCondition(String),
    
    #[error("Epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch { expected: u64, got: u64 },
    
    #[error("Not leader for this slot")]
    NotLeader,
}
