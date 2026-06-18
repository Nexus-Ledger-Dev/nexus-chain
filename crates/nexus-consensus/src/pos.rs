//! Proof of Stake implementation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use parking_lot::RwLock;
use sha3::{Digest, Keccak256};

use nexus_primitives::{Address, Hash, U256};
use crate::{ConsensusError, ConsensusResult, Validator, ValidatorSet};

/// Staking configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakingConfig {
    /// Minimum stake to become a validator
    pub min_validator_stake: u64,
    /// Minimum stake for delegation
    pub min_delegation: u64,
    /// Unbonding period in epochs
    pub unbonding_period: u64,
    /// Maximum commission rate (basis points)
    pub max_commission: u16,
    /// Max commission change per epoch (basis points)
    pub max_commission_change: u16,
    /// Slots per epoch
    pub slots_per_epoch: u64,
    /// Epoch duration in seconds
    pub epoch_duration: u64,
    /// Slash percentage for double signing (basis points)
    pub double_sign_slash: u64,
    /// Slash percentage for downtime (basis points)
    pub downtime_slash: u64,
    /// Jail duration for slashing (epochs)
    pub jail_duration: u64,
}

impl Default for StakingConfig {
    fn default() -> Self {
        Self {
            min_validator_stake: 10_000_000_000, // 10,000 tokens
            min_delegation: 1_000_000,           // 1 token
            unbonding_period: 14,                // 14 epochs (~14 days)
            max_commission: 5000,                // 50%
            max_commission_change: 100,          // 1% per epoch
            slots_per_epoch: 32,
            epoch_duration: 86400,               // 1 day
            double_sign_slash: 500,              // 5%
            downtime_slash: 100,                 // 1%
            jail_duration: 7,                    // 7 epochs
        }
    }
}

/// Delegation information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delegation {
    /// Delegator address
    pub delegator: Address,
    /// Validator address
    pub validator: Address,
    /// Delegated amount
    pub amount: u64,
    /// Shares in validator's pool
    pub shares: u64,
    /// Last claimed reward epoch
    pub last_claim_epoch: u64,
}

/// Unbonding entry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnbondingEntry {
    /// Delegator address
    pub delegator: Address,
    /// Validator address
    pub validator: Address,
    /// Amount unbonding
    pub amount: u64,
    /// Epoch when unbonding completes
    pub completion_epoch: u64,
}

/// Reward distribution
#[derive(Clone, Debug, Default)]
pub struct RewardPool {
    /// Total rewards accumulated
    pub total_rewards: u64,
    /// Rewards per share (scaled by 1e18)
    pub reward_per_share: U256,
    /// Epoch rewards were calculated for
    pub epoch: u64,
}

/// Proof of Stake manager
pub struct ProofOfStake {
    /// Configuration
    config: StakingConfig,
    /// Current epoch
    current_epoch: RwLock<u64>,
    /// Validator set
    validators: RwLock<ValidatorSet>,
    /// Delegations by delegator
    delegations: RwLock<HashMap<Address, Vec<Delegation>>>,
    /// Unbonding queue
    unbonding: RwLock<Vec<UnbondingEntry>>,
    /// Reward pools by validator
    reward_pools: RwLock<HashMap<Address, RewardPool>>,
}

impl ProofOfStake {
    /// Create new PoS manager
    pub fn new(config: StakingConfig) -> Self {
        Self {
            config,
            current_epoch: RwLock::new(0),
            validators: RwLock::new(ValidatorSet::new(0)),
            delegations: RwLock::new(HashMap::new()),
            unbonding: RwLock::new(Vec::new()),
            reward_pools: RwLock::new(HashMap::new()),
        }
    }
    
    /// Get current epoch
    pub fn epoch(&self) -> u64 {
        *self.current_epoch.read()
    }
    
    /// Register new validator
    pub fn register_validator(
        &self,
        address: Address,
        pubkey: Vec<u8>,
        stake: u64,
        commission: u16,
    ) -> ConsensusResult<()> {
        if stake < self.config.min_validator_stake {
            return Err(ConsensusError::InsufficientStake {
                required: self.config.min_validator_stake,
                available: stake,
            });
        }
        
        if commission > self.config.max_commission {
            return Err(ConsensusError::InvalidProposal(
                "Commission exceeds maximum".into()
            ));
        }
        
        let mut validator = Validator::new(address.clone(), pubkey, stake);
        validator.commission = commission;
        
        self.validators.write().add_validator(validator)?;
        
        // Initialize reward pool
        self.reward_pools.write().insert(address, RewardPool::default());
        
        Ok(())
    }
    
    /// Delegate stake to validator
    pub fn delegate(
        &self,
        delegator: Address,
        validator: Address,
        amount: u64,
    ) -> ConsensusResult<()> {
        if amount < self.config.min_delegation {
            return Err(ConsensusError::InsufficientStake {
                required: self.config.min_delegation,
                available: amount,
            });
        }
        
        // Share precision: 1 token = 1_000_000 base shares when the pool is empty.
        const SHARE_PRECISION: u64 = 1_000_000;

        // Read delegations BEFORE acquiring validators write lock to avoid inversion.
        let total_shares: u64 = {
            let dels = self.delegations.read();
            dels.values()
                .flatten()
                .filter(|d| d.validator == validator)
                .map(|d| d.shares)
                .sum()
        };

        let mut validators = self.validators.write();
        let v = validators.get_mut(&validator)
            .ok_or_else(|| ConsensusError::UnknownValidator(format!("{:?}", validator)))?;

        // Proportional share issuance: shares_issued = amount * total_shares / pool_stake.
        // When the pool is empty the ratio is 1:SHARE_PRECISION.
        let shares = if v.stake == 0 || total_shares == 0 {
            amount.saturating_mul(SHARE_PRECISION)
        } else {
            (amount as u128 * total_shares as u128 / v.stake as u128) as u64
        };

        v.stake += amount;
        validators.total_stake += amount;
        drop(validators);
        
        // Create delegation
        let delegation = Delegation {
            delegator: delegator.clone(),
            validator: validator.clone(),
            amount,
            shares,
            last_claim_epoch: self.epoch(),
        };
        
        self.delegations.write()
            .entry(delegator)
            .or_insert_with(Vec::new)
            .push(delegation);
        
        Ok(())
    }
    
    /// Undelegate stake
    pub fn undelegate(
        &self,
        delegator: &Address,
        validator: &Address,
        amount: u64,
    ) -> ConsensusResult<()> {
        let mut delegations = self.delegations.write();
        let dels = delegations.get_mut(delegator)
            .ok_or_else(|| ConsensusError::InvalidProposal("No delegation found".into()))?;
        
        let del = dels.iter_mut()
            .find(|d| &d.validator == validator)
            .ok_or_else(|| ConsensusError::InvalidProposal("No delegation to validator".into()))?;
        
        if del.amount < amount {
            return Err(ConsensusError::InsufficientStake {
                required: amount,
                available: del.amount,
            });
        }
        
        del.amount -= amount;
        del.shares = del.shares.saturating_sub(amount);
        
        // Remove if zero
        if del.amount == 0 {
            dels.retain(|d| &d.validator != validator);
        }
        drop(delegations);
        
        // Update validator stake
        let mut validators = self.validators.write();
        if let Some(v) = validators.get_mut(validator) {
            v.stake = v.stake.saturating_sub(amount);
            validators.total_stake = validators.total_stake.saturating_sub(amount);
        }
        drop(validators);
        
        // Add to unbonding queue
        let completion = self.epoch() + self.config.unbonding_period;
        self.unbonding.write().push(UnbondingEntry {
            delegator: delegator.clone(),
            validator: validator.clone(),
            amount,
            completion_epoch: completion,
        });
        
        Ok(())
    }
    
    /// Process epoch transition
    pub fn end_epoch(&self, rewards: u64) -> ConsensusResult<u64> {
        let mut epoch = self.current_epoch.write();
        *epoch += 1;
        let new_epoch = *epoch;
        drop(epoch);
        
        // Distribute rewards
        self.distribute_rewards(rewards)?;
        
        // Process unbonding completions
        self.process_unbonding(new_epoch)?;
        
        // Update validator set epoch
        self.validators.write().epoch = new_epoch;
        
        Ok(new_epoch)
    }
    
    /// Distribute epoch rewards to validators
    fn distribute_rewards(&self, total_rewards: u64) -> ConsensusResult<()> {
        let validators = self.validators.read();
        let total_stake = validators.active_stake();
        
        if total_stake == 0 {
            return Ok(());
        }
        
        let mut pools = self.reward_pools.write();
        
        for v in validators.active_validators() {
            // Proportional reward based on stake
            let validator_reward = (total_rewards as u128 * v.stake as u128 / total_stake as u128) as u64;
            
            // Commission goes to validator
            let commission = (validator_reward as u128 * v.commission as u128 / 10000) as u64;
            let delegator_reward = validator_reward - commission;
            
            // Update reward pool
            if let Some(pool) = pools.get_mut(&v.address) {
                pool.total_rewards += delegator_reward;
                if v.stake > 0 {
                    let reward_per_share_delta = U256::from(delegator_reward) * U256::from(1_000_000_000_000_000_000u64) / U256::from(v.stake);
                    pool.reward_per_share = pool.reward_per_share + reward_per_share_delta;
                }
                pool.epoch = self.epoch();
            }
        }
        
        Ok(())
    }
    
    /// Process completed unbonding
    fn process_unbonding(&self, epoch: u64) -> ConsensusResult<()> {
        let mut unbonding = self.unbonding.write();
        
        // Remove completed entries
        unbonding.retain(|entry| entry.completion_epoch > epoch);
        
        Ok(())
    }
    
    /// Claim pending rewards
    pub fn claim_rewards(&self, delegator: &Address) -> ConsensusResult<u64> {
        let current_epoch = self.epoch();
        let pools = self.reward_pools.read();
        let mut delegations = self.delegations.write();
        
        let dels = delegations.get_mut(delegator)
            .ok_or_else(|| ConsensusError::InvalidProposal("No delegations".into()))?;
        
        let mut total_claimed = 0u64;
        
        for del in dels.iter_mut() {
            if let Some(pool) = pools.get(&del.validator) {
                // Calculate pending rewards
                let pending = (U256::from(del.shares) * pool.reward_per_share / U256::from(1_000_000_000_000_000_000u64)).as_u64();
                total_claimed += pending;
                del.last_claim_epoch = current_epoch;
            }
        }
        
        Ok(total_claimed)
    }
    
    /// Get validator set
    pub fn validator_set(&self) -> ValidatorSet {
        self.validators.read().clone()
    }
    
    /// Get delegations for address
    pub fn get_delegations(&self, address: &Address) -> Vec<Delegation> {
        self.delegations.read()
            .get(address)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Get unbonding entries for address
    pub fn get_unbonding(&self, address: &Address) -> Vec<UnbondingEntry> {
        self.unbonding.read()
            .iter()
            .filter(|e| &e.delegator == address)
            .cloned()
            .collect()
    }
    
    /// Select leader for slot using verifiable random function
    pub fn select_leader(&self, epoch: u64, slot: u64, randomness: &Hash) -> Option<Address> {
        let validators = self.validators.read();
        let active = validators.active_validators();
        
        if active.is_empty() {
            return None;
        }
        
        // Create deterministic seed
        let mut seed_data = Vec::new();
        seed_data.extend_from_slice(&epoch.to_le_bytes());
        seed_data.extend_from_slice(&slot.to_le_bytes());
        seed_data.extend_from_slice(randomness.as_bytes());
        
        let seed = Keccak256::digest(&seed_data);
        let random_value = u64::from_le_bytes(seed[..8].try_into().unwrap());
        
        // Weighted selection by stake
        let total_stake = validators.active_stake();
        let target = random_value % total_stake;
        
        let mut cumulative = 0u64;
        for v in &active {
            cumulative += v.stake;
            if cumulative > target {
                return Some(v.address.clone());
            }
        }
        
        // Fallback to last validator
        active.last().map(|v| v.address.clone())
    }
}

impl Default for ProofOfStake {
    fn default() -> Self {
        Self::new(StakingConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_validator() {
        let pos = ProofOfStake::default();
        
        let result = pos.register_validator(
            Address::new([1u8; 20]),
            vec![0u8; 33],
            100_000_000_000,
            500,
        );
        
        assert!(result.is_ok());
        assert_eq!(pos.validator_set().len(), 1);
    }
    
    #[test]
    fn test_delegation() {
        let pos = ProofOfStake::default();
        
        pos.register_validator(
            Address::new([1u8; 20]),
            vec![0u8; 33],
            100_000_000_000,
            500,
        ).unwrap();
        
        pos.delegate(
            Address::new([2u8; 20]),
            Address::new([1u8; 20]),
            10_000_000,
        ).unwrap();
        
        let delegations = pos.get_delegations(&Address::new([2u8; 20]));
        assert_eq!(delegations.len(), 1);
        assert_eq!(delegations[0].amount, 10_000_000);
    }
    
    #[test]
    fn test_leader_selection() {
        let pos = ProofOfStake::default();
        
        for i in 0..3 {
            pos.register_validator(
                Address::new([i as u8; 20]),
                vec![0u8; 33],
                100_000_000_000,
                500,
            ).unwrap();
        }
        
        let leader = pos.select_leader(0, 0, &Hash::ZERO);
        assert!(leader.is_some());
    }
}
