//! Validator management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use k256::ecdsa::{SigningKey, VerifyingKey, signature::Signer};
use sha3::{Digest, Keccak256};

use nexus_primitives::{Address, Hash};
use crate::{ConsensusError, ConsensusResult};

/// Validator information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validator {
    /// Validator address
    pub address: Address,
    /// Public key (compressed, 33 bytes)
    pub pubkey: Vec<u8>,
    /// Total staked amount
    pub stake: u64,
    /// Commission rate (basis points, 100 = 1%)
    pub commission: u16,
    /// Whether validator is active
    pub active: bool,
    /// Jailed until epoch (0 = not jailed)
    pub jailed_until: u64,
    /// Total slashing events
    pub slash_count: u32,
    /// Uptime percentage (basis points)
    pub uptime: u16,
    /// Last active epoch
    pub last_active_epoch: u64,
}

impl Validator {
    /// Create new validator
    pub fn new(address: Address, pubkey: Vec<u8>, stake: u64) -> Self {
        Self {
            address,
            pubkey,
            stake,
            commission: 500, // 5% default
            active: true,
            jailed_until: 0,
            jailed_until: 0,
            slash_count: 0,
            uptime: 10000, // 100%
            last_active_epoch: 0,
        }
    }
    
    /// Check if validator is eligible to participate
    pub fn is_eligible(&self, current_epoch: u64) -> bool {
        self.active && self.jailed_until <= current_epoch && self.stake > 0
    }
    
    /// Voting power (proportional to stake)
    pub fn voting_power(&self) -> u64 {
        self.stake
    }
}

/// Validator set for a given epoch
#[derive(Clone, Debug)]
pub struct ValidatorSet {
    /// Current epoch
    pub epoch: u64,
    /// Active validators
    pub validators: HashMap<Address, Validator>,
    /// Total stake in the set
    pub total_stake: u64,
    /// Minimum stake required
    pub min_stake: u64,
    /// Maximum validators
    pub max_validators: usize,
}

impl ValidatorSet {
    /// Create empty validator set
    pub fn new(epoch: u64) -> Self {
        Self {
            epoch,
            validators: HashMap::new(),
            total_stake: 0,
            min_stake: 10_000_000_000, // 10,000 tokens (with 6 decimals)
            max_validators: 100,
        }
    }
    
    /// Add validator
    pub fn add_validator(&mut self, validator: Validator) -> ConsensusResult<()> {
        if validator.stake < self.min_stake {
            return Err(ConsensusError::InsufficientStake {
                required: self.min_stake,
                available: validator.stake,
            });
        }
        
        self.total_stake += validator.stake;
        self.validators.insert(validator.address.clone(), validator);
        Ok(())
    }
    
    /// Remove validator
    pub fn remove_validator(&mut self, address: &Address) -> Option<Validator> {
        if let Some(v) = self.validators.remove(address) {
            self.total_stake -= v.stake;
            Some(v)
        } else {
            None
        }
    }
    
    /// Get validator
    pub fn get(&self, address: &Address) -> Option<&Validator> {
        self.validators.get(address)
    }
    
    /// Get mutable validator
    pub fn get_mut(&mut self, address: &Address) -> Option<&mut Validator> {
        self.validators.get_mut(address)
    }
    
    /// Get all active validators
    pub fn active_validators(&self) -> Vec<&Validator> {
        self.validators.values()
            .filter(|v| v.is_eligible(self.epoch))
            .collect()
    }
    
    /// Total active stake
    pub fn active_stake(&self) -> u64 {
        self.active_validators().iter()
            .map(|v| v.stake)
            .sum()
    }
    
    /// Check if we have 2f+1 stake (required for BFT)
    pub fn has_supermajority(&self, stake: u64) -> bool {
        // Need > 2/3 of total stake
        stake * 3 > self.active_stake() * 2
    }
    
    /// Check if stake is at least f+1 (for liveness)
    pub fn has_quorum(&self, stake: u64) -> bool {
        // Need > 1/3 of total stake
        stake * 3 > self.active_stake()
    }
    
    /// Iterator over validators
    pub fn iter(&self) -> impl Iterator<Item = &Validator> {
        self.validators.values()
    }
    
    /// Number of validators
    pub fn len(&self) -> usize {
        self.validators.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }
}

/// Validator key management
pub struct ValidatorKeys {
    /// Signing key
    signing_key: SigningKey,
    /// Address derived from public key
    address: Address,
}

impl ValidatorKeys {
    /// Generate new validator keys
    pub fn generate() -> Self {
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let address = Self::derive_address(&signing_key);
        Self { signing_key, address }
    }
    
    /// Load from bytes
    pub fn from_bytes(bytes: &[u8]) -> ConsensusResult<Self> {
        let signing_key = SigningKey::from_bytes(bytes.into())
            .map_err(|_| ConsensusError::InvalidSignature)?;
        let address = Self::derive_address(&signing_key);
        Ok(Self { signing_key, address })
    }
    
    /// Derive address from signing key
    fn derive_address(key: &SigningKey) -> Address {
        let pubkey = key.verifying_key().to_encoded_point(false);
        let hash = Keccak256::digest(&pubkey.as_bytes()[1..]);
        Address::from_slice(&hash[12..])
    }
    
    /// Get address
    pub fn address(&self) -> &Address {
        &self.address
    }
    
    /// Get public key bytes (compressed)
    pub fn public_key(&self) -> Vec<u8> {
        self.signing_key.verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec()
    }
    
    /// Sign message
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let hash = Keccak256::digest(message);
        let signature: k256::ecdsa::Signature = self.signing_key.sign(&hash);
        signature.to_bytes().to_vec()
    }
    
    /// Sign hash directly
    pub fn sign_hash(&self, hash: &Hash) -> Vec<u8> {
        let signature: k256::ecdsa::Signature = self.signing_key.sign(hash.as_bytes());
        signature.to_bytes().to_vec()
    }
}

/// Validator registry (persistent)
pub struct ValidatorRegistry {
    /// Current validator set
    current_set: RwLock<ValidatorSet>,
    /// Next epoch's pending changes
    pending_changes: RwLock<Vec<ValidatorChange>>,
    /// Historical sets (for slashing evidence)
    historical_sets: RwLock<HashMap<u64, ValidatorSet>>,
}

/// Pending validator change
#[derive(Clone, Debug)]
pub enum ValidatorChange {
    /// New validator joining
    Join(Validator),
    /// Validator leaving
    Leave(Address),
    /// Stake increase
    StakeIncrease { address: Address, amount: u64 },
    /// Stake decrease (unbonding)
    StakeDecrease { address: Address, amount: u64 },
    /// Update commission
    UpdateCommission { address: Address, commission: u16 },
}

impl ValidatorRegistry {
    /// Create new registry
    pub fn new() -> Self {
        Self {
            current_set: RwLock::new(ValidatorSet::new(0)),
            pending_changes: RwLock::new(Vec::new()),
            historical_sets: RwLock::new(HashMap::new()),
        }
    }
    
    /// Get current validator set
    pub fn current_set(&self) -> ValidatorSet {
        self.current_set.read().clone()
    }
    
    /// Queue change for next epoch
    pub fn queue_change(&self, change: ValidatorChange) {
        self.pending_changes.write().push(change);
    }
    
    /// Apply pending changes and advance epoch
    pub fn advance_epoch(&self) -> ConsensusResult<u64> {
        let mut current = self.current_set.write();
        let changes = std::mem::take(&mut *self.pending_changes.write());
        
        // Store historical set
        self.historical_sets.write().insert(current.epoch, current.clone());
        
        // Apply changes
        for change in changes {
            match change {
                ValidatorChange::Join(v) => {
                    current.add_validator(v)?;
                }
                ValidatorChange::Leave(addr) => {
                    current.remove_validator(&addr);
                }
                ValidatorChange::StakeIncrease { address, amount } => {
                    if let Some(v) = current.get_mut(&address) {
                        v.stake += amount;
                        current.total_stake += amount;
                    }
                }
                ValidatorChange::StakeDecrease { address, amount } => {
                    if let Some(v) = current.get_mut(&address) {
                        v.stake = v.stake.saturating_sub(amount);
                        current.total_stake = current.total_stake.saturating_sub(amount);
                    }
                }
                ValidatorChange::UpdateCommission { address, commission } => {
                    if let Some(v) = current.get_mut(&address) {
                        v.commission = commission.min(10000);
                    }
                }
            }
        }
        
        current.epoch += 1;
        Ok(current.epoch)
    }
    
    /// Slash validator
    pub fn slash(&self, address: &Address, percentage: u64, jail_epochs: u64) -> ConsensusResult<u64> {
        let mut current = self.current_set.write();
        
        let validator = current.get_mut(address)
            .ok_or_else(|| ConsensusError::UnknownValidator(format!("{:?}", address)))?;
        
        // Calculate slash amount
        let slash_amount = (validator.stake * percentage) / 10000;
        validator.stake -= slash_amount;
        validator.slash_count += 1;
        validator.jailed_until = current.epoch + jail_epochs;
        
        current.total_stake -= slash_amount;
        
        Ok(slash_amount)
    }
    
    /// Get historical validator set
    pub fn get_historical_set(&self, epoch: u64) -> Option<ValidatorSet> {
        self.historical_sets.read().get(&epoch).cloned()
    }
}

impl Default for ValidatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validator_eligibility() {
        let mut v = Validator::new(
            Address::from([1u8; 20]),
            vec![0u8; 33],
            100_000_000_000,
        );
        
        assert!(v.is_eligible(0));
        
        v.jailed_until = 10;
        assert!(!v.is_eligible(5));
        assert!(v.is_eligible(10));
    }
    
    #[test]
    fn test_validator_set() {
        let mut set = ValidatorSet::new(0);
        
        let v1 = Validator::new(
            Address::from([1u8; 20]),
            vec![0u8; 33],
            100_000_000_000,
        );
        
        set.add_validator(v1).unwrap();
        
        assert_eq!(set.len(), 1);
        assert_eq!(set.total_stake, 100_000_000_000);
    }
    
    #[test]
    fn test_supermajority() {
        let mut set = ValidatorSet::new(0);
        
        for i in 0..3 {
            let v = Validator::new(
                Address::from([i as u8; 20]),
                vec![0u8; 33],
                100_000_000_000,
            );
            set.add_validator(v).unwrap();
        }
        
        // 2 out of 3 validators = 200B stake
        // Total = 300B
        // Need > 200B for supermajority
        assert!(!set.has_supermajority(200_000_000_000));
        assert!(set.has_supermajority(200_000_000_001));
    }
}
