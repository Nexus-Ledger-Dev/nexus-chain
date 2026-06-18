//! Core type definitions for NexusChain

use serde::{Deserialize, Serialize};
use std::fmt;

/// 32-byte hash type used throughout the system
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

// Duplicate Hash::new removed – use the implementation defined later

impl From<&[u8]> for Hash {
    fn from(slice: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&slice[0..32]);
        Hash::new(arr)
    }
}


impl Hash {
    pub const ZERO: Self = Self([0u8; 32]);
    
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(slice);
            Some(Self(bytes))
        } else {
            None
        }
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0[..8]))
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

/// Block/vertex height in the DAG (logical ordering)
pub type Height = u64;

/// Weight for DAG consensus (accumulated confidence)
pub type Weight = u128;

/// Timestamp in milliseconds since Unix epoch
pub type Timestamp = u64;

/// Nonce for transaction ordering and replay protection
pub type Nonce = u64;

/// Gas units for EVM execution
pub type Gas = u64;

/// Token amount (256-bit for EVM compatibility)
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct U256(pub [u64; 4]);

impl U256 {
    pub const ZERO: Self = Self([0, 0, 0, 0]);
    pub const ONE: Self = Self([1, 0, 0, 0]);
    
    pub fn from_u64(val: u64) -> Self {
        Self([val, 0, 0, 0])
    }
    
    pub fn from_be_bytes(bytes: [u8; 32]) -> Self {
        let mut result = [0u64; 4];
        for i in 0..4 {
            let offset = (3 - i) * 8;
            result[i] = u64::from_be_bytes([
                bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3],
                bytes[offset + 4], bytes[offset + 5], bytes[offset + 6], bytes[offset + 7],
            ]);
        }
        Self(result)
    }
    
    pub fn to_be_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for i in 0..4 {
            let offset = (3 - i) * 8;
            bytes[offset..offset + 8].copy_from_slice(&self.0[i].to_be_bytes());
        }
        bytes
    }
    
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        let mut result = [0u64; 4];
        let mut carry = 0u64;
        
        for i in 0..4 {
            let (sum1, overflow1) = self.0[i].overflowing_add(other.0[i]);
            let (sum2, overflow2) = sum1.overflowing_add(carry);
            result[i] = sum2;
            carry = (overflow1 as u64) + (overflow2 as u64);
        }
        
        if carry > 0 {
            None
        } else {
            Some(Self(result))
        }
    }
    
    pub fn checked_sub(&self, other: &Self) -> Option<Self> {
        let mut result = [0u64; 4];
        let mut borrow = 0u64;
        
        for i in 0..4 {
            let (diff1, overflow1) = self.0[i].overflowing_sub(other.0[i]);
            let (diff2, overflow2) = diff1.overflowing_sub(borrow);
            result[i] = diff2;
            borrow = (overflow1 as u64) + (overflow2 as u64);
        }
        
        if borrow > 0 {
            None
        } else {
            Some(Self(result))
        }
    }
}

// Basic arithmetic and conversion traits for U256 (very naive implementation)
use std::ops::{Add, Sub, Mul, Div};
use std::hash::{Hash as StdHash, Hasher};
use std::cmp::Ordering;

impl From<u64> for U256 {
    fn from(val: u64) -> Self { Self::from_u64(val) }
}

impl Add for U256 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self { self.checked_add(&rhs).expect("U256 overflow") }
}

impl Sub for U256 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { self.checked_sub(&rhs).expect("U256 underflow") }
}

impl Mul for U256 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        // Schoolbook multiply using u128 per cell to avoid intermediate overflow.
        // High bits beyond 256 are discarded (wrapping semantics, correct for EVM).
        let mut result = [0u64; 4];
        for i in 0..4 {
            let mut carry: u64 = 0;
            for j in 0..(4 - i) {
                let pos = i + j;
                // u64 * u64 + u64 + u64 fits in u128 (max ≈ 2^128 - 1)
                let product = (self.0[i] as u128) * (rhs.0[j] as u128)
                    + result[pos] as u128
                    + carry as u128;
                result[pos] = product as u64;
                carry = (product >> 64) as u64;
            }
            // carry past 256 bits is intentionally discarded
        }
        Self(result)
    }
}

impl Div for U256 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        self.checked_div(&rhs).expect("U256 division by zero")
    }
}

impl StdHash for U256 {
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.hash(state); }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        for i in (0..4).rev() {
            match self.0[i].cmp(&other.0[i]) {
                Ordering::Equal => continue,
                non_eq => return non_eq,
            }
        }
        Ordering::Equal
    }
}

impl U256 {
    pub fn as_u64(&self) -> u64 { self.0[0] }

    /// Checked division. Returns `None` on divide-by-zero.
    pub fn checked_div(&self, other: &Self) -> Option<Self> {
        if *other == Self::ZERO {
            return None;
        }
        if *self < *other {
            return Some(Self::ZERO);
        }
        // Bit-by-bit long division (constant time proportional to bit width).
        let mut quotient = Self::ZERO;
        let mut remainder = Self::ZERO;
        for i in (0..256).rev() {
            remainder = remainder.shl1();
            if self.bit(i) {
                remainder.0[0] |= 1;
            }
            if remainder >= *other {
                remainder = remainder.checked_sub(other).unwrap();
                quotient = quotient.set_bit(i);
            }
        }
        Some(quotient)
    }

    fn bit(&self, pos: usize) -> bool {
        (self.0[pos / 64] >> (pos % 64)) & 1 == 1
    }

    fn shl1(&self) -> Self {
        let mut r = [0u64; 4];
        let mut carry = 0u64;
        for i in 0..4 {
            r[i] = (self.0[i] << 1) | carry;
            carry = self.0[i] >> 63;
        }
        Self(r)
    }

    fn set_bit(mut self, pos: usize) -> Self {
        self.0[pos / 64] |= 1u64 << (pos % 64);
        self
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.to_be_bytes()))
    }
}

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::ZERO {
            return write!(f, "0");
        }
        let mut digits = Vec::new();
        let mut n = *self;
        let ten = Self::from(10u64);
        while n > Self::ZERO {
            let q = n.checked_div(&ten).unwrap();
            let rem = n.checked_sub(&(q * ten)).unwrap();
            digits.push(b'0' + rem.as_u64() as u8);
            n = q;
        }
        digits.reverse();
        f.write_str(std::str::from_utf8(&digits).unwrap())
    }
}

/// Validator/node identifier
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValidatorId(pub [u8; 32]);

impl ValidatorId {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Validator({})", hex::encode(&self.0[..8]))
    }
}

/// Version info for protocol upgrades
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolVersion {
    pub const CURRENT: Self = Self { major: 0, minor: 1, patch: 0 };
    
    pub fn is_compatible(&self, other: &Self) -> bool {
        self.major == other.major
    }
}

/// Chain configuration parameters (upgradeable via governance)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainConfig {
    /// Chain identifier
    pub chain_id: u64,
    
    /// Minimum gas price (in wei-equivalent)
    pub min_gas_price: U256,
    
    /// Maximum gas per transaction
    pub max_gas_per_tx: Gas,
    
    /// Block gas limit equivalent (for DAG vertex batches)
    pub vertex_gas_limit: Gas,
    
    /// Minimum stake to become validator
    pub min_validator_stake: U256,
    
    /// Maximum number of validators
    pub max_validators: u32,
    
    /// Finality threshold (BFT 2/3 + 1)
    pub finality_threshold: u32,
    
    /// Height at which various upgrades activate
    pub upgrade_heights: UpgradeHeights,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_id: 1337, // Default testnet
            min_gas_price: U256::from_u64(1_000_000_000), // 1 gwei
            max_gas_per_tx: 30_000_000,
            vertex_gas_limit: 100_000_000,
            min_validator_stake: U256::from_u64(10_000), // 10,000 tokens
            max_validators: 100,
            finality_threshold: 67, // 67% for BFT
            upgrade_heights: UpgradeHeights::default(),
        }
    }
}

/// Heights at which protocol upgrades activate
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpgradeHeights {
    /// ZK-EVM upgrade activation
    pub zk_evm: Option<Height>,
    
    /// ISO 20022 v2 message format
    pub iso_v2: Option<Height>,
    
    /// Enhanced privacy features
    pub privacy_v2: Option<Height>,
    
    /// Sharding activation
    pub sharding: Option<Height>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_u256_arithmetic() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(50);

        let sum = a.checked_add(&b).unwrap();
        assert_eq!(sum.0[0], 150);

        let diff = a.checked_sub(&b).unwrap();
        assert_eq!(diff.0[0], 50);
    }

    #[test]
    fn test_u256_mul_full_width() {
        // Verify multiplication that requires more than 128 bits of intermediate space.
        let a = U256([0, 1, 0, 0]); // 2^64
        let b = U256([0, 1, 0, 0]); // 2^64
        let product = a * b;        // 2^128 → lands in limb[2]
        assert_eq!(product, U256([0, 0, 1, 0]));

        // Simple sanity: 6 * 7 = 42
        assert_eq!(U256::from_u64(6) * U256::from_u64(7), U256::from_u64(42));
    }

    #[test]
    fn test_u256_div() {
        assert_eq!(U256::from_u64(100) / U256::from_u64(10), U256::from_u64(10));
        assert_eq!(U256::from_u64(7) / U256::from_u64(3), U256::from_u64(2));
        assert_eq!(U256::from_u64(0) / U256::from_u64(5), U256::ZERO);

        // Verify divide-by-zero returns None via checked_div
        assert!(U256::from_u64(1).checked_div(&U256::ZERO).is_none());

        // Wide value: 2^128 / 2^64 = 2^64
        let wide = U256([0, 0, 1, 0]); // 2^128
        let divisor = U256([0, 1, 0, 0]); // 2^64
        assert_eq!(wide / divisor, U256([0, 1, 0, 0]));
    }
    
    #[test]
    fn test_hash_display() {
        let hash = Hash::new([0xab; 32]);
        assert!(hash.to_string().starts_with("0x"));
    }
}
