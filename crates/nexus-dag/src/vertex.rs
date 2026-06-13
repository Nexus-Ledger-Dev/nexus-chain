//! Vertex (DAG node) implementation

use nexus_primitives::{
    Address, EcdsaSignature, Hash, Height, Timestamp, Transaction, ValidatorId, Weight,
    vertex_hash, compute_merkle_root,
};
use serde::{Deserialize, Serialize};

/// Maximum number of transactions per vertex
pub const MAX_TRANSACTIONS_PER_VERTEX: usize = 1000;

/// A vertex in the DAG (analogous to a block in traditional blockchains)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vertex {
    /// Unique identifier (hash of vertex contents)
    pub hash: Hash,
    
    /// References to parent vertices (enables DAG structure)
    pub parents: Vec<Hash>,
    
    /// Logical height (max parent height + 1)
    pub height: Height,
    
    /// Creation timestamp
    pub timestamp: Timestamp,
    
    /// Validator that created this vertex
    pub validator: ValidatorId,
    
    /// Transactions included in this vertex
    pub transactions: Vec<Transaction>,
    
    /// Merkle root of transactions
    pub tx_root: Hash,
    
    /// State root after applying this vertex
    pub state_root: Hash,
    
    /// Receipt root (for transaction receipts)
    pub receipts_root: Hash,
    
    /// Accumulated weight from this vertex's perspective
    pub weight: Weight,
    
    /// Validator signature
    pub signature: EcdsaSignature,
    
    /// Protocol version (for upgrades)
    pub version: u32,
    
    /// Extra data (validator-specific)
    pub extra_data: Vec<u8>,
}

impl Vertex {
    /// Create a new vertex (unsigned)
    pub fn new(
        parents: Vec<Hash>,
        height: Height,
        timestamp: Timestamp,
        validator: ValidatorId,
        transactions: Vec<Transaction>,
        state_root: Hash,
        receipts_root: Hash,
    ) -> Self {
        let tx_hashes: Vec<Hash> = transactions.iter()
            .map(|tx| tx.hash())
            .collect();
        
        let tx_root = compute_merkle_root(&tx_hashes);
        
        let hash = vertex_hash(
            &parents,
            &tx_hashes,
            timestamp,
            &validator.0,
        );
        
        Self {
            hash,
            parents,
            height,
            timestamp,
            validator,
            transactions,
            tx_root,
            state_root,
            receipts_root,
            weight: 1, // Base weight
            signature: EcdsaSignature { r: [0; 32], s: [0; 32], v: 0 },
            version: nexus_primitives::PROTOCOL_VERSION,
            extra_data: Vec::new(),
        }
    }
    
    /// Genesis vertex (root of the DAG)
    pub fn genesis(chain_id: u64, timestamp: Timestamp) -> Self {
        let validator = ValidatorId([0; 32]);
        
        Self {
            hash: Hash::new([0; 32]), // Will be recalculated
            parents: Vec::new(),
            height: 0,
            timestamp,
            validator,
            transactions: Vec::new(),
            tx_root: Hash::ZERO,
            state_root: Hash::ZERO,
            receipts_root: Hash::ZERO,
            weight: u128::MAX, // Maximum weight for genesis
            signature: EcdsaSignature { r: [0; 32], s: [0; 32], v: 0 },
            version: 1,
            extra_data: chain_id.to_be_bytes().to_vec(),
        }
    }
    
    /// Check if this is the genesis vertex
    pub fn is_genesis(&self) -> bool {
        self.parents.is_empty() && self.height == 0
    }
    
    /// Verify vertex structure (not signature)
    pub fn verify_structure(&self) -> Result<(), VertexError> {
        // Check parent count
        if !self.is_genesis() && self.parents.is_empty() {
            return Err(VertexError::NoParents);
        }
        
        if self.parents.len() > nexus_primitives::MAX_PARENTS {
            return Err(VertexError::TooManyParents(self.parents.len()));
        }
        
        // Check transaction count
        if self.transactions.len() > MAX_TRANSACTIONS_PER_VERTEX {
            return Err(VertexError::TooManyTransactions(self.transactions.len()));
        }
        
        // Verify transaction merkle root
        let tx_hashes: Vec<Hash> = self.transactions.iter()
            .map(|tx| tx.hash())
            .collect();
        let computed_root = compute_merkle_root(&tx_hashes);
        
        if computed_root != self.tx_root {
            return Err(VertexError::InvalidMerkleRoot);
        }
        
        // Verify hash
        let computed_hash = vertex_hash(
            &self.parents,
            &tx_hashes,
            self.timestamp,
            &self.validator.0,
        );
        
        if computed_hash != self.hash {
            return Err(VertexError::InvalidHash);
        }
        
        Ok(())
    }
    
    /// Calculate vertex size in bytes
    pub fn size(&self) -> usize {
        // Approximate size calculation
        let base_size = 32 + // hash
            self.parents.len() * 32 +
            8 + // height
            8 + // timestamp
            32 + // validator
            32 + // tx_root
            32 + // state_root
            32 + // receipts_root
            16 + // weight
            65 + // signature
            4 + // version
            self.extra_data.len();
        
        let tx_size: usize = self.transactions.iter()
            .map(|tx| tx.data.len() + 200) // Approximate tx overhead
            .sum();
        
        base_size + tx_size
    }
}

/// Vertex validation errors
#[derive(Debug, thiserror::Error)]
pub enum VertexError {
    #[error("Vertex has no parents (non-genesis)")]
    NoParents,
    
    #[error("Too many parents: {0} (max {})", nexus_primitives::MAX_PARENTS)]
    TooManyParents(usize),
    
    #[error("Too many transactions: {0} (max {})", MAX_TRANSACTIONS_PER_VERTEX)]
    TooManyTransactions(usize),
    
    #[error("Invalid merkle root")]
    InvalidMerkleRoot,
    
    #[error("Invalid vertex hash")]
    InvalidHash,
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Parent not found: {0}")]
    ParentNotFound(Hash),
    
    #[error("Timestamp too old")]
    TimestampTooOld,
    
    #[error("Timestamp in future")]
    TimestampInFuture,
    
    #[error("Invalid height: expected {expected}, got {got}")]
    InvalidHeight { expected: Height, got: Height },
}

/// Compact vertex representation for network transmission
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VertexHeader {
    pub hash: Hash,
    pub parents: Vec<Hash>,
    pub height: Height,
    pub timestamp: Timestamp,
    pub validator: ValidatorId,
    pub tx_root: Hash,
    pub state_root: Hash,
    pub receipts_root: Hash,
    pub weight: Weight,
    pub tx_count: u32,
}

impl From<&Vertex> for VertexHeader {
    fn from(vertex: &Vertex) -> Self {
        Self {
            hash: vertex.hash,
            parents: vertex.parents.clone(),
            height: vertex.height,
            timestamp: vertex.timestamp,
            validator: vertex.validator,
            tx_root: vertex.tx_root,
            state_root: vertex.state_root,
            receipts_root: vertex.receipts_root,
            weight: vertex.weight,
            tx_count: vertex.transactions.len() as u32,
        }
    }
}

/// Vertex status in the DAG
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexStatus {
    /// Just received, not validated
    Pending,
    
    /// Structure validated, awaiting parents
    AwaitingParents,
    
    /// Fully validated, included in DAG
    Confirmed,
    
    /// Finalized by BFT consensus
    Finalized,
    
    /// Rejected (invalid)
    Rejected,
    
    /// Orphaned (conflicting with finalized vertices)
    Orphaned,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_genesis_vertex() {
        let genesis = Vertex::genesis(1337, 1000);
        
        assert!(genesis.is_genesis());
        assert_eq!(genesis.height, 0);
        assert!(genesis.parents.is_empty());
    }
    
    #[test]
    fn test_vertex_structure_validation() {
        let genesis = Vertex::genesis(1337, 1000);
        
        // Genesis should pass (special case)
        // Note: Genesis hash validation would need special handling
        
        let mut invalid = genesis.clone();
        invalid.height = 1; // Non-genesis with no parents
        
        assert!(matches!(
            invalid.verify_structure(),
            Err(VertexError::NoParents)
        ));
    }
}
