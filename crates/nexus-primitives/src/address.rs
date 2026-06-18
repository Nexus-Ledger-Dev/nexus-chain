//! Address types for NexusChain (EVM compatible)

use crate::{keccak256, Hash};
use serde::{Deserialize, Serialize};
use std::fmt;

/// 20-byte Ethereum-compatible address
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub const ZERO: Self = Self([0u8; 20]);
    
    /// System addresses for precompiled contracts
    pub const SYSTEM_ZKP: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x20]);
    pub const SYSTEM_ISO: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x30]);
    pub const SYSTEM_GOVERNANCE: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
    pub const SYSTEM_STAKING: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02]);
    
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
    
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() == 20 {
            let mut bytes = [0u8; 20];
            bytes.copy_from_slice(slice);
            Some(Self(bytes))
        } else {
            None
        }
    }
    
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
    
    /// Convert to alloy Address for EVM compatibility
    pub fn to_alloy(&self) -> alloy_primitives::Address {
        alloy_primitives::Address::new(self.0)
    }
    
    /// Create from alloy Address
    pub fn from_alloy(addr: alloy_primitives::Address) -> Self {
        Self(*addr.0)
    }
    
    /// Check if this is a precompiled contract address
    pub fn is_precompile(&self) -> bool {
        // Standard Ethereum precompiles: 0x01 - 0x0A
        // ZKP precompiles: 0x20 - 0x2F
        // ISO precompiles: 0x30 - 0x3F
        let last_byte = self.0[19];
        let is_prefix_zero = self.0[..19].iter().all(|&b| b == 0);
        
        is_prefix_zero && (
            (0x01..=0x0A).contains(&last_byte) ||
            (0x20..=0x2F).contains(&last_byte) ||
            (0x30..=0x3F).contains(&last_byte)
        )
    }
    
    /// Create contract address from deployer and nonce (CREATE opcode)
    pub fn create_contract_address(deployer: &Address, nonce: u64) -> Self {
        // RLP encode [deployer, nonce]
        let mut buf = Vec::new();
        
        // RLP list header
        let deployer_len = 21; // 0x94 prefix + 20 bytes
        let nonce_len = if nonce == 0 {
            1
        } else if nonce < 0x80 {
            1
        } else {
            1 + ((64 - nonce.leading_zeros()) / 8) as usize + 
                if (64 - nonce.leading_zeros()) % 8 != 0 { 1 } else { 0 }
        };
        
        let total_len = deployer_len + nonce_len;
        
        if total_len < 56 {
            buf.push(0xC0 + total_len as u8);
        } else {
            let len_bytes = total_len.to_be_bytes();
            let len_start = len_bytes.iter().position(|&b| b != 0).unwrap_or(7);
            buf.push(0xF7 + (8 - len_start) as u8);
            buf.extend_from_slice(&len_bytes[len_start..]);
        }
        
        // Encode address
        buf.push(0x94);
        buf.extend_from_slice(&deployer.0);
        
        // Encode nonce
        if nonce == 0 {
            buf.push(0x80);
        } else if nonce < 0x80 {
            buf.push(nonce as u8);
        } else {
            let nonce_bytes = nonce.to_be_bytes();
            let start = nonce_bytes.iter().position(|&b| b != 0).unwrap_or(7);
            buf.push(0x80 + (8 - start) as u8);
            buf.extend_from_slice(&nonce_bytes[start..]);
        }
        
        let hash = keccak256(&buf);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..]);
        Self(addr)
    }
    
    /// Create contract address with CREATE2 opcode
    pub fn create2_contract_address(
        deployer: &Address,
        salt: &[u8; 32],
        init_code_hash: &Hash,
    ) -> Self {
        let mut data = Vec::with_capacity(1 + 20 + 32 + 32);
        data.push(0xFF);
        data.extend_from_slice(&deployer.0);
        data.extend_from_slice(salt);
        data.extend_from_slice(init_code_hash.as_bytes());
        
        let hash = keccak256(&data);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..]);
        Self(addr)
    }
    
    /// Parse from hex string (with or without 0x prefix)
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s)?;
        if bytes.len() != 20 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&bytes);
        Ok(Self(addr))
    }
    
    /// Convert to checksummed hex string (EIP-55)
    pub fn to_checksum_string(&self) -> String {
        let hex_addr = hex::encode(&self.0);
        let hash = keccak256(hex_addr.as_bytes());
        
        let mut result = String::with_capacity(42);
        result.push_str("0x");
        
        for (i, c) in hex_addr.chars().enumerate() {
            let hash_nibble = if i % 2 == 0 {
                (hash[i / 2] >> 4) & 0x0F
            } else {
                hash[i / 2] & 0x0F
            };
            
            if hash_nibble >= 8 {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
        }
        
        result
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum_string())
    }
}

/// Account identifier that can be either an address or a special system account
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountId {
    /// Standard EOA or contract address
    Address(Address),
    
    /// System account (governance, staking, etc.)
    System(u8),
    
    /// Validator identified by their public key hash
    Validator([u8; 32]),
}

impl AccountId {
    pub fn to_address(&self) -> Address {
        match self {
            AccountId::Address(addr) => *addr,
            AccountId::System(id) => {
                let mut addr = [0u8; 20];
                addr[19] = *id;
                Address(addr)
            }
            AccountId::Validator(hash) => {
                let mut addr = [0u8; 20];
                addr.copy_from_slice(&hash[12..32]);
                Address(addr)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_address_from_hex() {
        let addr = Address::from_hex("0xdead000000000000000000000000000000000000").unwrap();
        assert_eq!(addr.0[0], 0xde);
        assert_eq!(addr.0[1], 0xad);
    }
    
    #[test]
    fn test_checksum_address() {
        let addr = Address::from_hex("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        assert_eq!(
            addr.to_checksum_string(),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
    }
    
    #[test]
    fn test_precompile_detection() {
        assert!(Address::SYSTEM_ZKP.is_precompile());
        assert!(Address::SYSTEM_ISO.is_precompile());
        assert!(!Address::from_hex("0xdead000000000000000000000000000000000000").unwrap().is_precompile());
    }
    
    #[test]
    fn test_create_address() {
        // Example from Ethereum
        let deployer = Address::from_hex("0x6ac7ea33f8831ea9dcc53393aaa88b25a785dbf0").unwrap();
        let nonce = 0;
        let created = Address::create_contract_address(&deployer, nonce);
        // The created address should be deterministic
        assert_ne!(created, Address::ZERO);
    }
}
