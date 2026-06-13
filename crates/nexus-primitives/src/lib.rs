//! # NexusChain Primitives
//!
//! Core types and cryptographic primitives used throughout the NexusChain ecosystem.
//! This module is designed for maximum modularity - all other crates depend on this
//! but this crate has minimal dependencies.

mod types;
mod crypto;
mod transaction;
mod address;
mod error;

pub use types::*;
pub use crypto::*;
pub use transaction::*;
pub use address::*;
pub use error::*;

/// Protocol version for upgrade coordination
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum transaction size in bytes
pub const MAX_TX_SIZE: usize = 128 * 1024; // 128 KB

/// Maximum number of parent references in DAG
pub const MAX_PARENTS: usize = 8;

/// Precompile address ranges (EVM compatibility)
pub mod precompiles {
    use alloy_primitives::Address;
    
    /// ZKP verification precompiles (0x20 - 0x2F)
    pub const ZKP_VERIFY_GROTH16: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x20]);
    pub const ZKP_VERIFY_PLONK: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x21]);
    pub const ZKP_POSEIDON_HASH: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x22]);
    pub const ZKP_COMMITMENT: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x23]);
    
    /// ISO compliance precompiles (0x30 - 0x3F)  
    pub const ISO_20022_VALIDATE: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x30]);
    pub const ISO_8583_PARSE: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x31]);
    pub const LEI_VALIDATE: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x32]);
    pub const IBAN_VALIDATE: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x33]);
}
