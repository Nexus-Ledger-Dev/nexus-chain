//! State checkpointing for DAG state management

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use nexus_primitives::{Address, Hash, U256};
use crate::{StateDb, StateResult, StateError};

/// Checkpoint identifier
pub type CheckpointId = u64;

/// State change record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateChange {
    BalanceChange {
        address: Address,
        old_value: U256,
        new_value: U256,
    },
    NonceChange {
        address: Address,
        old_value: u64,
        new_value: u64,
    },
    CodeChange {
        address: Address,
        old_code: Vec<u8>,
        new_code: Vec<u8>,
    },
    StorageChange {
        address: Address,
        key: U256,
        old_value: U256,
        new_value: U256,
    },
    AccountCreated {
        address: Address,
    },
    AccountDeleted {
        address: Address,
        balance: U256,
        nonce: u64,
    },
}

/// Checkpoint containing state changes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint ID
    pub id: CheckpointId,
    /// Vertex hash this checkpoint is for
    pub vertex_hash: Hash,
    /// State root after applying changes
    pub state_root: Hash,
    /// List of changes
    pub changes: Vec<StateChange>,
    /// Parent checkpoint(s) - multiple for DAG
    pub parents: Vec<CheckpointId>,
}

/// Checkpoint manager for DAG-based state
pub struct CheckpointManager {
    /// All checkpoints
    checkpoints: RwLock<HashMap<CheckpointId, Checkpoint>>,
    /// Vertex hash to checkpoint mapping
    vertex_to_checkpoint: RwLock<HashMap<Hash, CheckpointId>>,
    /// Next checkpoint ID
    next_id: RwLock<CheckpointId>,
    /// Finalized checkpoint
    finalized_checkpoint: RwLock<CheckpointId>,
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            checkpoints: RwLock::new(HashMap::new()),
            vertex_to_checkpoint: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
            finalized_checkpoint: RwLock::new(0),
        }
    }
    
    /// Create a new checkpoint
    pub fn create_checkpoint(
        &self,
        vertex_hash: Hash,
        state_root: Hash,
        changes: Vec<StateChange>,
        parent_vertices: &[Hash],
    ) -> CheckpointId {
        let mut next_id = self.next_id.write();
        let id = *next_id;
        *next_id += 1;
        
        // Find parent checkpoints
        let vertex_map = self.vertex_to_checkpoint.read();
        let parents: Vec<CheckpointId> = parent_vertices.iter()
            .filter_map(|h| vertex_map.get(h).copied())
            .collect();
        drop(vertex_map);
        
        let checkpoint = Checkpoint {
            id,
            vertex_hash: vertex_hash.clone(),
            state_root,
            changes,
            parents,
        };
        
        self.checkpoints.write().insert(id, checkpoint);
        self.vertex_to_checkpoint.write().insert(vertex_hash, id);
        
        id
    }
    
    /// Get checkpoint by ID
    pub fn get_checkpoint(&self, id: CheckpointId) -> Option<Checkpoint> {
        self.checkpoints.read().get(&id).cloned()
    }
    
    /// Get checkpoint for vertex
    pub fn get_checkpoint_for_vertex(&self, vertex_hash: &Hash) -> Option<Checkpoint> {
        let id = self.vertex_to_checkpoint.read().get(vertex_hash).copied()?;
        self.get_checkpoint(id)
    }
    
    /// Get state root at checkpoint
    pub fn get_state_root(&self, id: CheckpointId) -> Option<Hash> {
        self.checkpoints.read().get(&id).map(|c| c.state_root.clone())
    }
    
    /// Mark checkpoint as finalized
    pub fn finalize(&self, id: CheckpointId) -> StateResult<()> {
        if !self.checkpoints.read().contains_key(&id) {
            return Err(StateError::Checkpoint("Checkpoint not found".into()));
        }
        
        *self.finalized_checkpoint.write() = id;
        
        // Prune old checkpoints before finalized
        self.prune_before(id);
        
        Ok(())
    }
    
    /// Get finalized checkpoint
    pub fn finalized(&self) -> CheckpointId {
        *self.finalized_checkpoint.read()
    }
    
    /// Prune checkpoints before ID
    fn prune_before(&self, before_id: CheckpointId) {
        let mut checkpoints = self.checkpoints.write();
        let mut vertex_map = self.vertex_to_checkpoint.write();
        
        // Find checkpoints to remove
        let to_remove: Vec<_> = checkpoints.iter()
            .filter(|(id, _)| **id < before_id)
            .map(|(id, cp)| (*id, cp.vertex_hash.clone()))
            .collect();
        
        for (id, hash) in to_remove {
            checkpoints.remove(&id);
            vertex_map.remove(&hash);
        }
    }
    
    /// Apply checkpoint changes to state
    pub fn apply_checkpoint(&self, state: &StateDb, id: CheckpointId) -> StateResult<()> {
        let checkpoint = self.get_checkpoint(id)
            .ok_or_else(|| StateError::Checkpoint("Checkpoint not found".into()))?;
        
        for change in &checkpoint.changes {
            match change {
                StateChange::BalanceChange { address, new_value, .. } => {
                    state.set_balance(address, new_value.clone());
                }
                StateChange::NonceChange { address, new_value, .. } => {
                    state.set_nonce(address, *new_value);
                }
                StateChange::CodeChange { address, new_code, .. } => {
                    state.set_code(address, new_code.clone());
                }
                StateChange::StorageChange { address, key, new_value, .. } => {
                    state.set_storage(address, key.clone(), new_value.clone());
                }
                StateChange::AccountCreated { address } => {
                    // Account already exists if we're applying changes
                }
                StateChange::AccountDeleted { address, .. } => {
                    state.delete_account(address);
                }
            }
        }
        
        Ok(())
    }
    
    /// Revert checkpoint changes from state
    pub fn revert_checkpoint(&self, state: &StateDb, id: CheckpointId) -> StateResult<()> {
        let checkpoint = self.get_checkpoint(id)
            .ok_or_else(|| StateError::Checkpoint("Checkpoint not found".into()))?;
        
        // Apply changes in reverse order, using old values
        for change in checkpoint.changes.iter().rev() {
            match change {
                StateChange::BalanceChange { address, old_value, .. } => {
                    state.set_balance(address, old_value.clone());
                }
                StateChange::NonceChange { address, old_value, .. } => {
                    state.set_nonce(address, *old_value);
                }
                StateChange::CodeChange { address, old_code, .. } => {
                    state.set_code(address, old_code.clone());
                }
                StateChange::StorageChange { address, key, old_value, .. } => {
                    state.set_storage(address, key.clone(), old_value.clone());
                }
                StateChange::AccountCreated { address } => {
                    state.delete_account(address);
                }
                StateChange::AccountDeleted { address, balance, nonce } => {
                    state.set_balance(address, balance.clone());
                    state.set_nonce(address, *nonce);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get checkpoint chain to finalized
    pub fn get_chain_to_finalized(&self, from: CheckpointId) -> Vec<CheckpointId> {
        let finalized = self.finalized();
        let mut chain = Vec::new();
        let mut current = from;
        
        while current > finalized {
            chain.push(current);
            if let Some(cp) = self.get_checkpoint(current) {
                current = cp.parents.first().copied().unwrap_or(0);
            } else {
                break;
            }
        }
        
        chain.reverse();
        chain
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_checkpoint_manager() {
        let manager = CheckpointManager::new();
        
        let changes = vec![
            StateChange::BalanceChange {
                address: Address::from([1u8; 20]),
                old_value: U256::ZERO,
                new_value: U256::from(100),
            },
        ];
        
        let id = manager.create_checkpoint(
            Hash::from([1u8; 32]),
            Hash::from([2u8; 32]),
            changes,
            &[],
        );
        
        assert_eq!(id, 1);
        
        let cp = manager.get_checkpoint(id).unwrap();
        assert_eq!(cp.changes.len(), 1);
    }
}
