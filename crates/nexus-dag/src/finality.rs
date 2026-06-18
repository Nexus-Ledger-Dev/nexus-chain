//! BFT Finality Gadget for DAG Consensus
//!
//! While the DAG provides high throughput and parallel processing,
//! deterministic finality requires a BFT consensus overlay. This module
//! implements a GRANDPA-inspired finality mechanism adapted for DAGs.

use crate::Dag;
use nexus_primitives::{
    Address, EcdsaSignature, Hash, Height, NexusError, PublicKey, Result,
    SecretKey, ValidatorId, Weight,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

/// Finality configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinalityConfig {
    /// Minimum validators for finality (should be 2/3 + 1 of total)
    pub min_validators: usize,
    
    /// Timeout for vote collection (milliseconds)
    pub vote_timeout_ms: u64,
    
    /// Maximum rounds without progress before view change
    pub max_stall_rounds: u32,
    
    /// Finality check interval (heights)
    pub finality_check_interval: Height,
}

impl Default for FinalityConfig {
    fn default() -> Self {
        Self {
            min_validators: 3,
            vote_timeout_ms: 5000,
            max_stall_rounds: 10,
            finality_check_interval: 10,
        }
    }
}

/// Validator set for finality voting
#[derive(Clone, Debug)]
pub struct ValidatorSet {
    /// Active validators and their weights
    validators: HashMap<ValidatorId, ValidatorInfo>,
    
    /// Total weight
    total_weight: Weight,
    
    /// Epoch number
    epoch: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub id: ValidatorId,
    #[serde(with = "serde_bytes")]
    pub public_key: [u8; 64], // Uncompressed public key bytes
    pub stake: Weight,
    pub address: Address,
}

impl ValidatorSet {
    pub fn new(validators: Vec<ValidatorInfo>, epoch: u64) -> Self {
        let total_weight = validators.iter().map(|v| v.stake).sum();
        let validators = validators.into_iter()
            .map(|v| (v.id, v))
            .collect();
        
        Self {
            validators,
            total_weight,
            epoch,
        }
    }
    
    /// Check if we have enough weight for finality (2/3 + 1)
    pub fn has_supermajority(&self, weight: Weight) -> bool {
        // Prevent division by zero
        if self.total_weight == 0 {
            return false;
        }
        // 2/3 + 1 threshold
        weight * 3 > self.total_weight * 2
    }
    
    pub fn get_validator(&self, id: &ValidatorId) -> Option<&ValidatorInfo> {
        self.validators.get(id)
    }
    
    pub fn validators(&self) -> impl Iterator<Item = &ValidatorInfo> {
        self.validators.values()
    }
    
    pub fn len(&self) -> usize {
        self.validators.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }
}

/// Vote for finality
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinalityVote {
    /// Round number
    pub round: u64,
    
    /// Target vertex hash
    pub target_hash: Hash,
    
    /// Target height
    pub target_height: Height,
    
    /// Voter ID
    pub voter: ValidatorId,
    
    /// Signature over (round, target_hash, target_height)
    pub signature: EcdsaSignature,
}

impl FinalityVote {
    pub fn new(
        round: u64,
        target_hash: Hash,
        target_height: Height,
        voter: ValidatorId,
        secret_key: &SecretKey,
    ) -> Result<Self> {
        let message = Self::signing_message(round, &target_hash, target_height);
        let message_hash = nexus_primitives::blake3_hash(&message);
        let signature = secret_key.sign(&message_hash)?;
        
        Ok(Self {
            round,
            target_hash,
            target_height,
            voter,
            signature,
        })
    }
    
    pub fn signing_message(round: u64, target: &Hash, height: Height) -> Vec<u8> {
        let mut message = Vec::with_capacity(48);
        message.extend_from_slice(&round.to_be_bytes());
        message.extend_from_slice(target.as_bytes());
        message.extend_from_slice(&height.to_be_bytes());
        message
    }
    
    pub fn verify(&self, public_key: &PublicKey) -> Result<bool> {
        let message = Self::signing_message(self.round, &self.target_hash, self.target_height);
        let message_hash = nexus_primitives::blake3_hash(&message);
        nexus_primitives::verify_signature(&message_hash, &self.signature, public_key)
    }
}

/// Finality proof (aggregated votes)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinalityProof {
    /// Round number
    pub round: u64,
    
    /// Finalized vertex hash
    pub target_hash: Hash,
    
    /// Finalized height
    pub target_height: Height,
    
    /// Participating validators and their votes
    pub votes: Vec<FinalityVote>,
    
    /// Total weight of votes
    pub total_weight: Weight,
    
    /// Epoch of the validator set
    pub epoch: u64,
}

impl FinalityProof {
    /// Verify the finality proof against a validator set
    pub fn verify(&self, validator_set: &ValidatorSet) -> Result<bool> {
        if validator_set.epoch != self.epoch {
            return Err(NexusError::Consensus("Epoch mismatch".into()));
        }
        
        let mut verified_weight: Weight = 0;
        let mut seen_voters = HashSet::new();
        
        for vote in &self.votes {
            // Check for duplicate voters
            if !seen_voters.insert(vote.voter) {
                return Err(NexusError::Consensus("Duplicate voter".into()));
            }
            
            // Check vote matches proof target
            if vote.target_hash != self.target_hash || vote.target_height != self.target_height {
                return Err(NexusError::Consensus("Vote target mismatch".into()));
            }
            
            // Get validator info
            let validator = validator_set.get_validator(&vote.voter)
                .ok_or_else(|| NexusError::Consensus("Unknown validator".into()))?;
            
            // Verify signature
            let public_key = PublicKey(validator.public_key);
            if !vote.verify(&public_key)? {
                return Err(NexusError::Consensus("Invalid vote signature".into()));
            }
            
            verified_weight += validator.stake;
        }
        
        // Check supermajority
        if !validator_set.has_supermajority(verified_weight) {
            return Err(NexusError::Consensus("Insufficient vote weight".into()));
        }
        
        Ok(true)
    }
}

/// Finality tracker state
#[derive(Clone, Debug)]
pub struct FinalityState {
    /// Current round
    pub round: u64,
    
    /// Last finalized height
    pub finalized_height: Height,
    
    /// Last finalized hash
    pub finalized_hash: Hash,
    
    /// Current votes
    pub votes: HashMap<ValidatorId, FinalityVote>,
    
    /// Best candidate for finalization
    pub best_candidate: Option<(Hash, Height)>,
}

/// The finality gadget
pub struct FinalityGadget {
    #[allow(dead_code)]
    config: FinalityConfig,
    state: RwLock<FinalityState>,
    validator_set: RwLock<ValidatorSet>,
    proofs: RwLock<Vec<FinalityProof>>,
}

impl FinalityGadget {
    pub fn new(config: FinalityConfig, initial_validators: Vec<ValidatorInfo>) -> Self {
        let validator_set = ValidatorSet::new(initial_validators, 0);
        
        Self {
            config,
            state: RwLock::new(FinalityState {
                round: 0,
                finalized_height: 0,
                finalized_hash: Hash::ZERO,
                votes: HashMap::new(),
                best_candidate: None,
            }),
            validator_set: RwLock::new(validator_set),
            proofs: RwLock::new(Vec::new()),
        }
    }
    
    /// Process a finality vote
    pub fn add_vote(&self, vote: FinalityVote) -> Result<Option<FinalityProof>> {
        let mut state = self.state.write();
        let validator_set = self.validator_set.read();
        
        // Verify vote is for current or future round
        if vote.round < state.round {
            return Err(NexusError::Consensus("Vote for past round".into()));
        }
        
        // Verify voter is a validator
        let validator = validator_set.get_validator(&vote.voter)
            .ok_or_else(|| NexusError::Consensus("Unknown validator".into()))?;
        
        // Verify signature
        let public_key = PublicKey(validator.public_key);
        if !vote.verify(&public_key)? {
            return Err(NexusError::Consensus("Invalid vote signature".into()));
        }
        
        // Store vote
        state.votes.insert(vote.voter, vote.clone());
        
        debug!(
            "Received vote for height {} from {:?}, total votes: {}",
            vote.target_height,
            vote.voter,
            state.votes.len()
        );
        
        // Check if we can finalize
        self.try_finalize(&mut state, &validator_set)
    }
    
    /// Attempt to finalize based on current votes
    fn try_finalize(
        &self,
        state: &mut FinalityState,
        validator_set: &ValidatorSet,
    ) -> Result<Option<FinalityProof>> {
        // Group votes by target
        let mut target_weights: HashMap<(Hash, Height), (Weight, Vec<FinalityVote>)> = HashMap::new();
        
        for vote in state.votes.values() {
            if let Some(validator) = validator_set.get_validator(&vote.voter) {
                let key = (vote.target_hash, vote.target_height);
                let entry = target_weights.entry(key).or_insert((0, Vec::new()));
                entry.0 += validator.stake;
                entry.1.push(vote.clone());
            }
        }
        
        // Find target with supermajority
        for ((hash, height), (weight, votes)) in target_weights {
            if validator_set.has_supermajority(weight) && height > state.finalized_height {
                // Create finality proof
                let proof = FinalityProof {
                    round: state.round,
                    target_hash: hash,
                    target_height: height,
                    votes,
                    total_weight: weight,
                    epoch: validator_set.epoch,
                };
                
                // Update state
                state.finalized_height = height;
                state.finalized_hash = hash;
                state.round += 1;
                state.votes.clear();
                
                info!("Finalized height {} with weight {}", height, weight);
                
                // Store proof
                self.proofs.write().push(proof.clone());
                
                return Ok(Some(proof));
            }
        }
        
        Ok(None)
    }
    
    /// Get current finalized state
    pub fn finalized_state(&self) -> (Height, Hash) {
        let state = self.state.read();
        (state.finalized_height, state.finalized_hash)
    }
    
    /// Get the best finalization candidate from DAG
    pub fn find_best_candidate(&self, dag: &Dag) -> Option<(Hash, Height)> {
        let state = self.state.read();
        let current_height = dag.latest_height();
        
        // Find the highest vertex that is an ancestor of all tips
        let tips = dag.get_tips();
        
        if tips.is_empty() {
            return None;
        }
        
        // For simplicity, find common ancestors
        // A more sophisticated implementation would use LCA algorithms
        for height in (state.finalized_height + 1..=current_height).rev() {
            let vertices_at_height = dag.get_vertices_at_height(height);
            
            for vertex_hash in vertices_at_height {
                // Check if this vertex is an ancestor of all tips
                let is_common_ancestor = tips.iter()
                    .all(|tip| dag.is_ancestor(&vertex_hash, tip) || *tip == vertex_hash);
                
                if is_common_ancestor {
                    return Some((vertex_hash, height));
                }
            }
        }
        
        None
    }
    
    /// Create a vote for the best candidate
    pub fn create_vote(
        &self,
        dag: &Dag,
        voter_id: ValidatorId,
        secret_key: &SecretKey,
    ) -> Result<Option<FinalityVote>> {
        let state = self.state.read();
        
        if let Some((hash, height)) = self.find_best_candidate(dag) {
            let vote = FinalityVote::new(
                state.round,
                hash,
                height,
                voter_id,
                secret_key,
            )?;
            
            return Ok(Some(vote));
        }
        
        Ok(None)
    }
    
    /// Update validator set (for epoch transitions)
    pub fn update_validator_set(&self, validators: Vec<ValidatorInfo>, epoch: u64) {
        let new_set = ValidatorSet::new(validators, epoch);
        *self.validator_set.write() = new_set;
        
        // Clear votes on epoch change
        let mut state = self.state.write();
        state.votes.clear();
        
        info!("Validator set updated for epoch {}", epoch);
    }
    
    /// Get finality proofs for a height range
    pub fn get_proofs(&self, from_height: Height, to_height: Height) -> Vec<FinalityProof> {
        self.proofs.read()
            .iter()
            .filter(|p| p.target_height >= from_height && p.target_height <= to_height)
            .cloned()
            .collect()
    }
    
    /// Verify and apply a finality proof (for syncing nodes)
    pub fn apply_proof(&self, proof: FinalityProof, dag: &Dag) -> Result<()> {
        // Verify the proof
        let validator_set = self.validator_set.read();
        proof.verify(&validator_set)?;
        
        // Verify the target exists in DAG
        if dag.get_vertex(&proof.target_hash).is_none() {
            return Err(NexusError::Consensus("Proof target not found in DAG".into()));
        }
        
        // Update state
        let mut state = self.state.write();
        
        if proof.target_height > state.finalized_height {
            state.finalized_height = proof.target_height;
            state.finalized_hash = proof.target_hash;
            
            // Update DAG finality
            dag.set_finalized_height(proof.target_height);
            
            // Store proof
            drop(state);
            self.proofs.write().push(proof);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_validators(count: usize) -> Vec<(ValidatorInfo, SecretKey)> {
        (0..count)
            .map(|i| {
                let secret = SecretKey::generate();
                let public = PublicKey::from_secret(&secret).unwrap();
                let id = ValidatorId([i as u8; 32]);
                
                let info = ValidatorInfo {
                    id,
                    public_key: public.0,
                    stake: 100,
                    address: public.to_address(),
                };
                
                (info, secret)
            })
            .collect()
    }
    
    #[test]
    fn test_supermajority_calculation() {
        let validators: Vec<ValidatorInfo> = create_test_validators(4)
            .into_iter()
            .map(|(v, _)| v)
            .collect();
        
        let set = ValidatorSet::new(validators, 0);
        
        // Total weight: 400
        // 2/3 + 1 = 267
        assert!(!set.has_supermajority(200)); // 50%
        assert!(!set.has_supermajority(266)); // Just under
        assert!(set.has_supermajority(267));  // Exactly 2/3 + 1
        assert!(set.has_supermajority(300));  // 75%
    }
    
    #[test]
    fn test_vote_creation_and_verification() {
        let (validator, secret) = &create_test_validators(1)[0];
        
        let vote = FinalityVote::new(
            1,
            Hash::new([0xAB; 32]),
            100,
            validator.id,
            secret,
        ).unwrap();
        
        let public_key = PublicKey(validator.public_key);
        assert!(vote.verify(&public_key).unwrap());
    }
    
    #[test]
    fn test_finality_gadget_basic() {
        let validators_with_keys = create_test_validators(4);
        let validators: Vec<ValidatorInfo> = validators_with_keys.iter()
            .map(|(v, _)| v.clone())
            .collect();
        
        let config = FinalityConfig::default();
        let gadget = FinalityGadget::new(config, validators);
        
        let target_hash = Hash::new([0xAB; 32]);
        let target_height = 100;
        
        // Add votes from 3 validators (75%, above 2/3)
        for (validator, secret) in validators_with_keys.iter().take(3) {
            let vote = FinalityVote::new(
                0,
                target_hash,
                target_height,
                validator.id,
                secret,
            ).unwrap();
            
            let result = gadget.add_vote(vote);
            
            if let Ok(Some(proof)) = result {
                // Should finalize after 3rd vote
                assert_eq!(proof.target_height, target_height);
                assert_eq!(proof.target_hash, target_hash);
                return;
            }
        }
        
        // Should have finalized
        let (height, _) = gadget.finalized_state();
        assert_eq!(height, target_height);
    }
}
