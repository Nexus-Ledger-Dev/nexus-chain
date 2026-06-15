//! BFT Consensus for DAG finality

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use once_cell::sync::OnceCell;
use std::sync::Mutex;
use crate::iso_logger::IsoLogger;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::mpsc;

static LOGGER: OnceCell<Mutex<IsoLogger>> = OnceCell::new();
use sha3::{Digest, Keccak256};

use nexus_primitives::{Address, Hash};
use crate::{ConsensusError, ConsensusResult, ValidatorSet, ValidatorKeys};

/// BFT message types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BftMessage {
    /// Proposal for new vertex/block
    Proposal(Proposal),
    /// Prepare vote
    Prepare(PrepareVote),
    /// Commit vote
    Commit(CommitVote),
    /// View change request
    ViewChange(ViewChangeRequest),
    /// New view notification
    NewView(NewViewMessage),
}

/// Block/Vertex proposal
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    /// Proposer address
    pub proposer: Address,
    /// View number
    pub view: u64,
    /// Epoch
    pub epoch: u64,
    /// Vertex/block hash
    pub vertex_hash: Hash,
    /// Parent hashes (for DAG)
    pub parents: Vec<Hash>,
    /// Signature
    pub signature: Vec<u8>,
}

impl Proposal {
    /// Create signing hash
    pub fn signing_hash(&self) -> Hash {
        let mut data = Vec::new();
        data.extend_from_slice(&self.view.to_le_bytes());
        data.extend_from_slice(&self.epoch.to_le_bytes());
        data.extend_from_slice(self.vertex_hash.as_bytes());
        for parent in &self.parents {
            data.extend_from_slice(parent.as_bytes());
        }
        Hash::new(Keccak256::digest(&data).into())
    }
}

/// Prepare vote
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrepareVote {
    /// Voter address
    pub voter: Address,
    /// View number
    pub view: u64,
    /// Vertex hash being voted on
    pub vertex_hash: Hash,
    /// Signature
    pub signature: Vec<u8>,
}

impl PrepareVote {
    pub fn signing_hash(&self) -> Hash {
        let mut data = Vec::new();
        data.push(0x01); // Prepare prefix
        data.extend_from_slice(&self.view.to_le_bytes());
        data.extend_from_slice(self.vertex_hash.as_bytes());
        Hash::new(Keccak256::digest(&data).into())
    }
}

/// Commit vote
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitVote {
    /// Voter address
    pub voter: Address,
    /// View number
    pub view: u64,
    /// Vertex hash
    pub vertex_hash: Hash,
    /// Signature
    pub signature: Vec<u8>,
}

impl CommitVote {
    pub fn signing_hash(&self) -> Hash {
        let mut data = Vec::new();
        data.push(0x02); // Commit prefix
        data.extend_from_slice(&self.view.to_le_bytes());
        data.extend_from_slice(self.vertex_hash.as_bytes());
        Hash::new(Keccak256::digest(&data).into())
    }
}

/// View change request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewChangeRequest {
    /// Requester address
    pub requester: Address,
    /// New view number
    pub new_view: u64,
    /// Last prepared vertex
    pub last_prepared: Option<Hash>,
    /// Signature
    pub signature: Vec<u8>,
}

/// New view message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewViewMessage {
    /// Leader address
    pub leader: Address,
    /// New view number
    pub view: u64,
    /// View change certificates
    pub view_changes: Vec<ViewChangeRequest>,
    /// Signature
    pub signature: Vec<u8>,
}

/// BFT round state
#[derive(Clone, Debug)]
pub struct BftRoundState {
    /// Current view
    pub view: u64,
    /// Current epoch
    pub epoch: u64,
    /// Phase
    pub phase: BftPhase,
    /// Current proposal
    pub proposal: Option<Proposal>,
    /// Prepare votes received
    pub prepare_votes: HashMap<Address, PrepareVote>,
    /// Commit votes received
    pub commit_votes: HashMap<Address, CommitVote>,
    /// Locked vertex (prepared)
    pub locked_vertex: Option<Hash>,
    /// Locked view
    pub locked_view: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BftPhase {
    /// Waiting for proposal
    NewRound,
    /// Proposal received, collecting prepares
    Prepare,
    /// 2f+1 prepares received, collecting commits
    Commit,
    /// 2f+1 commits received, finalized
    Finalized,
}

impl Default for BftRoundState {
    fn default() -> Self {
        Self {
            view: 0,
            epoch: 0,
            phase: BftPhase::NewRound,
            proposal: None,
            prepare_votes: HashMap::new(),
            commit_votes: HashMap::new(),
            locked_vertex: None,
            locked_view: 0,
        }
    }
}

/// BFT consensus engine
pub struct BftConsensus {
    /// Validator keys
    keys: ValidatorKeys,
    /// Validator set
    validator_set: Arc<RwLock<ValidatorSet>>,
    /// Current round state
    state: RwLock<BftRoundState>,
    /// Finalized vertices
    finalized: RwLock<HashSet<Hash>>,
    /// View change votes
    view_change_votes: RwLock<HashMap<u64, Vec<ViewChangeRequest>>>,
    /// Message sender
    message_tx: Option<mpsc::Sender<BftMessage>>,
}

impl BftConsensus {
    /// Public setter for test harnesses – replaces the internal round state.
    pub fn set_state(&self, state: BftRoundState) {
        *self.state.write() = state;
    }
    /// Create new BFT consensus
    pub fn new(keys: ValidatorKeys, validator_set: Arc<RwLock<ValidatorSet>>) -> Self {
        Self {
            keys,
            validator_set,
            state: RwLock::new(BftRoundState::default()),
            finalized: RwLock::new(HashSet::new()),
            view_change_votes: RwLock::new(HashMap::new()),
            message_tx: None,
        }
    }
    
    /// Set message channel
    pub fn set_message_channel(&mut self, tx: mpsc::Sender<BftMessage>) {
        self.message_tx = Some(tx);
    }
    
    /// Check if we're the leader for current view
    pub fn is_leader(&self) -> bool {
        let state = self.state.read();
        let validators = self.validator_set.read();
        
        // Simple round-robin leader selection
        let active: Vec<_> = validators.active_validators()
            .iter()
            .map(|v| v.address.clone())
            .collect();
        
        if active.is_empty() {
            return false;
        }
        
        let leader_idx = (state.view as usize) % active.len();
        active.get(leader_idx) == Some(self.keys.address())
    }

    /// Weighted leader selection based on stake (deterministic)
    pub fn weighted_leader(&self) -> Option<Address> {
        let validators = self.validator_set.read();
        let active = validators.active_validators();
        let total_stake: u128 = active.iter().map(|v| v.stake as u128).sum();
        if total_stake == 0 {
            return None;
        }
        // deterministic hash of view and epoch
        let state = self.state.read();
        let mut hasher = Keccak256::new();
        hasher.update(&state.view.to_le_bytes());
        hasher.update(&state.epoch.to_le_bytes());
        let rand = u128::from_be_bytes(hasher.finalize()[..16].try_into().unwrap());
        let target = rand % total_stake;
        let mut cum: u128 = 0;
        for v in active {
            cum += v.stake as u128;
            if cum > target {
                return Some(v.address.clone());
            }
        }
        None
    }

    /// Check if the given address is the weighted leader for the current view
    pub fn is_weighted_leader(&self, my_addr: &Address) -> bool {
        match self.weighted_leader() {
            Some(leader) => &leader == my_addr,
            None => false,
        }
    }
    
    /// Create and broadcast proposal
    pub fn propose(&self, vertex_hash: Hash, parents: Vec<Hash>) -> ConsensusResult<Proposal> {
        if !self.is_leader() {
            return Err(ConsensusError::NotLeader);
        }
        
        let state = self.state.read();
        
        let proposal = Proposal {
            proposer: self.keys.address().clone(),
            view: state.view,
            epoch: state.epoch,
            vertex_hash,
            parents,
            signature: vec![], // Filled below
        };
        
        let mut proposal = proposal;
        proposal.signature = self.keys.sign_hash(&proposal.signing_hash());
        
        drop(state);
        
        // Update state
        let mut state = self.state.write();
        state.proposal = Some(proposal.clone());
        state.phase = BftPhase::Prepare;
        
        // Also vote for own proposal
        drop(state);
        self.vote_prepare(&vertex_hash)?;
        
        Ok(proposal)
    }
    
    /// Handle received proposal
    pub fn on_proposal(&self, proposal: Proposal) -> ConsensusResult<()> {
        // Verify proposer is leader
        let state = self.state.read();
        
        if proposal.view != state.view {
            return Err(ConsensusError::InvalidProposal(
                format!("View mismatch: expected {}, got {}", state.view, proposal.view)
            ));
        }
        
        // Check if locked (safety)
        if let Some(locked) = &state.locked_vertex {
            if proposal.vertex_hash != *locked && proposal.view <= state.locked_view {
                return Err(ConsensusError::InvalidProposal(
                    "Proposal conflicts with locked vertex".into()
                ));
            }
        }
        
        drop(state);
        
        // TODO: Verify signature
        
        // Accept proposal
        let mut state = self.state.write();
        state.proposal = Some(proposal.clone());
        state.phase = BftPhase::Prepare;
        drop(state);
        
        // Vote prepare
        self.vote_prepare(&proposal.vertex_hash)?;
        
        Ok(())
    }
    
    /// Vote prepare for vertex
    pub fn vote_prepare(&self, vertex_hash: &Hash) -> ConsensusResult<()> {
        let state = self.state.read();
        
        let vote = PrepareVote {
            voter: self.keys.address().clone(),
            view: state.view,
            vertex_hash: vertex_hash.clone(),
            signature: self.keys.sign_hash(&Hash::new(Keccak256::digest(&[&[0x01], state.view.to_le_bytes().as_slice(), vertex_hash.as_bytes()].concat()).into())),
        };
        
        drop(state);
        
        self.on_prepare(vote)
    }
    
    /// Handle prepare vote
    pub fn on_prepare(&self, vote: PrepareVote) -> ConsensusResult<()> {
        let validators = self.validator_set.read();
        
        // Verify voter is validator
        let validator = validators.get(&vote.voter)
            .ok_or_else(|| ConsensusError::UnknownValidator(format!("{:?}", vote.voter)))?;
        
        let stake = validator.stake;
        drop(validators);
        
        // TODO: Verify signature
        
        // Add vote
        let mut state = self.state.write();
        
        if state.phase != BftPhase::Prepare {
            return Ok(());
        }
        
        // Check for duplicate
        if state.prepare_votes.contains_key(&vote.voter) {
            return Err(ConsensusError::DuplicateVote);
        }
        
        let vertex_hash = vote.vertex_hash.clone();
        state.prepare_votes.insert(vote.voter.clone(), vote);
        
        // Check if we have 2f+1 prepare votes
        let validators = self.validator_set.read();
        let prepare_stake: u64 = state.prepare_votes.values()
            .filter_map(|v| validators.get(&v.voter))
            .map(|v| v.stake)
            .sum();
        
        if validators.has_supermajority(prepare_stake) {
            // Lock on this vertex
            state.locked_vertex = Some(vertex_hash.clone());
            state.locked_view = state.view;
            state.phase = BftPhase::Commit;
            
            drop(state);
            drop(validators);
            
            // Vote commit
            self.vote_commit(&vertex_hash)?;
        }
        
        Ok(())
    }
    
    /// Vote commit for vertex
    pub fn vote_commit(&self, vertex_hash: &Hash) -> ConsensusResult<()> {
        let state = self.state.read();
        
        let vote = CommitVote {
                    voter: self.keys.address().clone(),
                    view: state.view,
                    vertex_hash: vertex_hash.clone(),
                    signature: self.keys.sign_hash(&Hash::new(Keccak256::digest(&[&[0x02], state.view.to_le_bytes().as_slice(), vertex_hash.as_bytes()].concat()).into()))
                };
        drop(state);
        
        self.on_commit(vote).map(|_| ())
    }
    
    /// Handle commit vote
    pub fn on_commit(&self, vote: CommitVote) -> ConsensusResult<Option<Hash>> {
        let validators = self.validator_set.read();
        
        // Verify voter
        validators.get(&vote.voter)
            .ok_or_else(|| ConsensusError::UnknownValidator(format!("{:?}", vote.voter)))?;
        
        // TODO: Verify signature
        
        let mut state = self.state.write();
        
        if state.phase != BftPhase::Commit {
            return Ok(None);
        }
        
        if state.commit_votes.contains_key(&vote.voter) {
            return Err(ConsensusError::DuplicateVote);
        }
        
        let vertex_hash = vote.vertex_hash.clone();
        state.commit_votes.insert(vote.voter.clone(), vote);
        
        // Check if we have 2f+1 commit votes
        let commit_stake: u64 = state.commit_votes.values()
            .filter_map(|v| validators.get(&v.voter))
            .map(|v| v.stake)
            .sum();
        
        if validators.has_supermajority(commit_stake) {
            state.phase = BftPhase::Finalized;
            
            drop(state);
            drop(validators);
            
            // Mark as finalized
            self.finalized.write().insert(vertex_hash.clone());
            
            // Advance view
            self.advance_view();
            
            return Ok(Some(vertex_hash));
        }
        
        Ok(None)
    }
    
    /// Request view change
    pub fn request_view_change(&self, new_view: u64) -> ConsensusResult<()> {
        let state = self.state.read();
        
        let request = ViewChangeRequest {
            requester: self.keys.address().clone(),
            new_view,
            last_prepared: state.locked_vertex.clone(),
            signature: self.keys.sign(&new_view.to_le_bytes()),
        };
        
        drop(state);
        
        self.on_view_change(request)
    }
    
    /// Handle view change request
    pub fn on_view_change(&self, request: ViewChangeRequest) -> ConsensusResult<()> {
        let validators = self.validator_set.read();
        
        validators.get(&request.requester)
            .ok_or_else(|| ConsensusError::UnknownValidator(format!("{:?}", request.requester)))?;
        
        let mut vc_votes = self.view_change_votes.write();
        vc_votes.entry(request.new_view)
            .or_insert_with(Vec::new)
            .push(request.clone());
        
        // Check if we have 2f+1 view change requests
        let votes = vc_votes.get(&request.new_view).unwrap();
        let vc_stake: u64 = votes.iter()
            .filter_map(|v| validators.get(&v.requester))
            .map(|v| v.stake)
            .sum();
        
        if validators.has_supermajority(vc_stake) {
            drop(vc_votes);
            drop(validators);
            
            // Perform view change
            let mut state = self.state.write();
            state.view = request.new_view;
            state.phase = BftPhase::NewRound;
            state.proposal = None;
            state.prepare_votes.clear();
            state.commit_votes.clear();
        }
        
        Ok(())
    }
    
    /// Advance to next view
    pub fn advance_view(&self) {
        let mut state = self.state.write();
        state.view += 1;
        state.phase = BftPhase::NewRound;
        state.proposal = None;
        state.prepare_votes.clear();
        state.commit_votes.clear();
    }
    
    /// Check if vertex is finalized
    pub fn is_finalized(&self, hash: &Hash) -> bool {
        self.finalized.read().contains(hash)
    }
    
    /// Get current view
    pub fn current_view(&self) -> u64 {
        self.state.read().view
    }
    
    /// Get current phase
    pub fn current_phase(&self) -> BftPhase {
        self.state.read().phase
    }
    
    /// Get finality certificate (2f+1 commits)
    pub fn get_finality_certificate(&self, hash: &Hash) -> Option<Vec<CommitVote>> {
        let state = self.state.read();
        
        let commits: Vec<_> = state.commit_votes.values()
            .filter(|v| &v.vertex_hash == hash)
            .cloned()
            .collect();
        
        let validators = self.validator_set.read();
        let stake: u64 = commits.iter()
            .filter_map(|v| validators.get(&v.voter))
            .map(|v| v.stake)
            .sum();
        
        if validators.has_supermajority(stake) {
            Some(commits)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_bft() -> (BftConsensus, Arc<RwLock<ValidatorSet>>) {
        let keys = ValidatorKeys::generate();
        let mut validator_set = ValidatorSet::new(0);
        
        // Add self as validator
        let validator = crate::Validator::new(
            keys.address().clone(),
            keys.public_key(),
            100_000_000_000,
        );
        validator_set.add_validator(validator).unwrap();
        
        let validator_set = Arc::new(RwLock::new(validator_set));
        let bft = BftConsensus::new(keys, validator_set.clone());
        
        (bft, validator_set)
    }
    
    #[test]
    fn test_single_validator_consensus() {
        let (bft, _) = setup_bft();
        
        // As sole validator, we should be leader
        assert!(bft.is_leader());
        
        // Propose
        let hash = Hash::new([1u8; 32]);
        let proposal = bft.propose(hash.clone(), vec![]).unwrap();
        
        assert_eq!(proposal.vertex_hash, hash);
        
        // Should be finalized (we're the only validator)
        // After prepare -> commit -> finalize
        assert!(bft.is_finalized(&hash));
    }
}
