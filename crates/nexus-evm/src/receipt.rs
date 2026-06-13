//! Transaction execution receipts

use serde::{Deserialize, Serialize};
use nexus_primitives::{Address, Hash, U256};

/// Log entry (event)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Log {
    /// Contract address
    pub address: Address,
    /// Indexed topics
    pub topics: Vec<Hash>,
    /// Event data
    pub data: Vec<u8>,
}

/// Transaction execution status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Transaction succeeded
    Success,
    /// Transaction reverted
    Revert,
    /// Transaction failed (out of gas, etc.)
    Failure,
}

impl ExecutionStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionStatus::Success)
    }
}

/// Transaction receipt
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    /// Transaction hash
    pub tx_hash: Hash,
    /// Transaction index in block/vertex
    pub tx_index: u32,
    /// Vertex hash (DAG equivalent of block hash)
    pub vertex_hash: Hash,
    /// Execution status
    pub status: ExecutionStatus,
    /// Cumulative gas used up to and including this tx
    pub cumulative_gas_used: u64,
    /// Gas used by this transaction
    pub gas_used: u64,
    /// Effective gas price
    pub effective_gas_price: U256,
    /// Contract address created (if contract creation)
    pub contract_address: Option<Address>,
    /// Logs emitted
    pub logs: Vec<Log>,
    /// Logs bloom filter
    pub logs_bloom: Bloom,
    /// Revert reason (if reverted)
    pub revert_reason: Option<String>,
}

impl Receipt {
    /// Create a success receipt
    pub fn success(
        tx_hash: Hash,
        vertex_hash: Hash,
        gas_used: u64,
        effective_gas_price: U256,
        logs: Vec<Log>,
    ) -> Self {
        let logs_bloom = Bloom::from_logs(&logs);
        Self {
            tx_hash,
            tx_index: 0,
            vertex_hash,
            status: ExecutionStatus::Success,
            cumulative_gas_used: gas_used,
            gas_used,
            effective_gas_price,
            contract_address: None,
            logs,
            logs_bloom,
            revert_reason: None,
        }
    }
    
    /// Create a revert receipt
    pub fn revert(
        tx_hash: Hash,
        vertex_hash: Hash,
        gas_used: u64,
        effective_gas_price: U256,
        reason: String,
    ) -> Self {
        Self {
            tx_hash,
            tx_index: 0,
            vertex_hash,
            status: ExecutionStatus::Revert,
            cumulative_gas_used: gas_used,
            gas_used,
            effective_gas_price,
            contract_address: None,
            logs: vec![],
            logs_bloom: Bloom::default(),
            revert_reason: Some(reason),
        }
    }
    
    /// Create a failure receipt
    pub fn failure(
        tx_hash: Hash,
        vertex_hash: Hash,
        gas_used: u64,
        effective_gas_price: U256,
    ) -> Self {
        Self {
            tx_hash,
            tx_index: 0,
            vertex_hash,
            status: ExecutionStatus::Failure,
            cumulative_gas_used: gas_used,
            gas_used,
            effective_gas_price,
            contract_address: None,
            logs: vec![],
            logs_bloom: Bloom::default(),
            revert_reason: None,
        }
    }
}

/// Bloom filter for log indexing (2048 bits)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bloom(pub [u8; 256]);

impl Bloom {
    /// Create bloom filter from logs
    pub fn from_logs(logs: &[Log]) -> Self {
        let mut bloom = Self::default();
        
        for log in logs {
            bloom.accrue_address(&log.address);
            for topic in &log.topics {
                bloom.accrue_hash(topic);
            }
        }
        
        bloom
    }
    
    /// Add address to bloom
    pub fn accrue_address(&mut self, address: &Address) {
        let hash = sha3::Keccak256::digest(address.as_bytes());
        self.accrue_raw(&hash);
    }
    
    /// Add hash to bloom
    pub fn accrue_hash(&mut self, hash: &Hash) {
        let hash = sha3::Keccak256::digest(hash.as_bytes());
        self.accrue_raw(&hash);
    }
    
    fn accrue_raw(&mut self, hash: &[u8]) {
        // Use first 6 bytes to set 3 bits
        for i in 0..3 {
            let bit_index = ((hash[i * 2] as usize) << 8 | (hash[i * 2 + 1] as usize)) % 2048;
            let byte_index = bit_index / 8;
            let bit_position = bit_index % 8;
            self.0[byte_index] |= 1 << bit_position;
        }
    }
    
    /// Check if bloom contains address
    pub fn contains_address(&self, address: &Address) -> bool {
        let hash = sha3::Keccak256::digest(address.as_bytes());
        self.contains_raw(&hash)
    }
    
    /// Check if bloom contains hash
    pub fn contains_hash(&self, hash: &Hash) -> bool {
        let h = sha3::Keccak256::digest(hash.as_bytes());
        self.contains_raw(&h)
    }
    
    fn contains_raw(&self, hash: &[u8]) -> bool {
        for i in 0..3 {
            let bit_index = ((hash[i * 2] as usize) << 8 | (hash[i * 2 + 1] as usize)) % 2048;
            let byte_index = bit_index / 8;
            let bit_position = bit_index % 8;
            if self.0[byte_index] & (1 << bit_position) == 0 {
                return false;
            }
        }
        true
    }
    
    /// Combine blooms (OR)
    pub fn combine(&mut self, other: &Bloom) {
        for i in 0..256 {
            self.0[i] |= other.0[i];
        }
    }
}

/// Execution result
#[derive(Clone, Debug)]
pub struct ExecutionResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Gas used
    pub gas_used: u64,
    /// Gas refunded
    pub gas_refunded: u64,
    /// Output data (return value or revert data)
    pub output: Vec<u8>,
    /// Logs emitted
    pub logs: Vec<Log>,
    /// Contract created (for CREATE/CREATE2)
    pub created_address: Option<Address>,
}

impl ExecutionResult {
    /// Successful execution
    pub fn success(gas_used: u64, gas_refunded: u64, output: Vec<u8>, logs: Vec<Log>) -> Self {
        Self {
            success: true,
            gas_used,
            gas_refunded,
            output,
            logs,
            created_address: None,
        }
    }
    
    /// Reverted execution
    pub fn revert(gas_used: u64, output: Vec<u8>) -> Self {
        Self {
            success: false,
            gas_used,
            gas_refunded: 0,
            output,
            logs: vec![],
            created_address: None,
        }
    }
    
    /// Get revert reason if available
    pub fn revert_reason(&self) -> Option<String> {
        if self.success || self.output.len() < 4 {
            return None;
        }
        
        // Check for Error(string) selector: 0x08c379a0
        if &self.output[0..4] != &[0x08, 0xc3, 0x79, 0xa0] {
            return None;
        }
        
        // Skip selector + offset + length (4 + 32 + 32 = 68 bytes)
        if self.output.len() < 68 {
            return None;
        }
        
        // Get string length
        let len_offset = 36; // 4 + 32
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&self.output[len_offset + 24..len_offset + 32]);
        let len = u64::from_be_bytes(len_bytes) as usize;
        
        let string_offset = 68;
        if self.output.len() < string_offset + len {
            return None;
        }
        
        String::from_utf8(self.output[string_offset..string_offset + len].to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_filter() {
        let mut bloom = Bloom::default();
        let addr = Address::from([0x42u8; 20]);
        
        assert!(!bloom.contains_address(&addr));
        bloom.accrue_address(&addr);
        assert!(bloom.contains_address(&addr));
    }
    
    #[test]
    fn test_receipt_success() {
        let receipt = Receipt::success(
            Hash::ZERO,
            Hash::ZERO,
            21000,
            U256::from(1_000_000_000),
            vec![],
        );
        
        assert!(receipt.status.is_success());
        assert_eq!(receipt.gas_used, 21000);
    }
}
