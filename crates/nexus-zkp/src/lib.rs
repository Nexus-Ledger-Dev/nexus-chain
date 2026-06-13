//! # NexusChain Zero-Knowledge Proof Module
//!
//! This module provides ZKP capabilities for privacy-preserving transactions
//! and compliance verification. It supports:
//!
//! - **Private Transactions**: Hide transaction amounts while proving validity
//! - **Compliance Proofs**: Prove KYC/AML compliance without revealing identity
//! - **State Proofs**: Enable light clients to verify state transitions
//! - **Range Proofs**: Prove values are within valid ranges
//!
//! ## Cryptographic Primitives
//!
//! - Groth16 proofs (BN254 curve) for efficiency
//! - Poseidon hash for ZK-friendly hashing
//! - Pedersen commitments for value hiding
//!
//! ## Regulatory Compliance
//!
//! The ZKP system is designed to support selective disclosure:
//! - Users can prove compliance without revealing all data
//! - Regulators can be given viewing keys for audit
//! - GDPR "right to erasure" supported via commitment schemes

mod circuits;
mod prover;
mod verifier;
mod commitment;
mod poseidon;
mod trait_verifier;

pub use circuits::*;
pub use prover::*;
pub use verifier::*;
pub use commitment::*;
pub use poseidon::*;
pub use trait_verifier::*;

/// ZKP system configuration
#[derive(Clone, Debug)]
pub struct ZkpConfig {
    /// Maximum supported transaction value (for range proofs)
    pub max_value: u64,
    
    /// Number of bits for range proofs
    pub range_bits: usize,
    
    /// Merkle tree depth for commitments
    pub tree_depth: usize,
}

impl Default for ZkpConfig {
    fn default() -> Self {
        Self {
            max_value: u64::MAX,
            range_bits: 64,
            tree_depth: 32,
        }
    }
}

/// ZKP error types
#[derive(Debug, thiserror::Error)]
pub enum ZkpError {
    #[error("Proof generation failed: {0}")]
    ProofGenerationFailed(String),
    
    #[error("Proof verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Circuit error: {0}")]
    CircuitError(String),
    
    #[error("Invalid commitment")]
    InvalidCommitment,
    
    #[error("Nullifier already spent")]
    NullifierSpent,
}

pub type ZkpResult<T> = Result<T, ZkpError>;
