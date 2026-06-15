//! EVM State Management
//! 
//! Provides state management for EVM execution with DAG-aware features.

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use std::sync::Arc;

use nexus_primitives::{Address, Hash, U256};
use crate::{EvmError, EvmResult};

/// Account state
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    /// Account nonce
    pub nonce: u64,
    /// Account balance
    pub balance: U256,
    /// Code hash (keccak256 of code)
    pub code_hash: Hash,
    /// Storage root (merkle root of storage trie)
    pub storage_root: Hash,
}

impl Account {
    /// Empty account constant
    pub const EMPTY: Account = Account {
        nonce: 0,
        balance: U256::ZERO,
        code_hash: Hash::ZERO,
        storage_root: Hash::ZERO,
    };
    
    /// Check if account is empty (EIP-161)
    pub fn is_empty(&self) -> bool {
        self.nonce == 0 && self.balance == U256::ZERO && self.code_hash == Hash::ZERO
    }
}

/// Storage slot key-value pair
pub type StorageSlot = (U256, U256);

/// EVM state database
pub struct StateDb {
    /// Account states
    accounts: DashMap<Address, Account>,
    /// Contract code
    code: DashMap<Hash, Vec<u8>>,
    /// Account storage
    storage: DashMap<Address, HashMap<U256, U256>>,
    /// State root
    state_root: RwLock<Hash>,
    /// Dirty accounts (modified since last commit)
    dirty_accounts: DashMap<Address, ()>,
    /// Dirty storage
    dirty_storage: DashMap<Address, Vec<U256>>,
}

impl StateDb {
    /// Create new empty state
    pub fn new() -> Self {
        Self {
            accounts: DashMap::new(),
            code: DashMap::new(),
            storage: DashMap::new(),
            state_root: RwLock::new(Hash::ZERO),
            dirty_accounts: DashMap::new(),
            dirty_storage: DashMap::new(),
        }
    }
    
    /// Get account
    pub fn get_account(&self, address: &Address) -> Account {
        self.accounts.get(address)
            .map(|a| a.clone())
            .unwrap_or_default()
    }
    
    /// Set account
    pub fn set_account(&self, address: Address, account: Account) {
        self.accounts.insert(address.clone(), account);
        self.dirty_accounts.insert(address, ());
    }
    
    /// Get account balance
    pub fn get_balance(&self, address: &Address) -> U256 {
        self.get_account(address).balance
    }
    
    /// Set account balance
    pub fn set_balance(&self, address: &Address, balance: U256) {
        let mut account = self.get_account(address);
        account.balance = balance;
        self.set_account(address.clone(), account);
    }
    
    /// Add to account balance
    pub fn add_balance(&self, address: &Address, amount: U256) -> EvmResult<()> {
        let account = self.get_account(address);
        let new_balance = account.balance.checked_add(amount)
            .ok_or_else(|| EvmError::StateError("Balance overflow".into()))?;
        self.set_balance(address, new_balance);
        Ok(())
    }
    
    /// Subtract from account balance
    pub fn sub_balance(&self, address: &Address, amount: U256) -> EvmResult<()> {
        let account = self.get_account(address);
        if account.balance < amount {
            return Err(EvmError::InsufficientBalance {
                required: format!("{:?}", amount),
                available: format!("{:?}", account.balance),
            });
        }
        let new_balance = account.balance - amount;
        self.set_balance(address, new_balance);
        Ok(())
    }
    
    /// Get account nonce
    pub fn get_nonce(&self, address: &Address) -> u64 {
        self.get_account(address).nonce
    }
    
    /// Increment account nonce
    pub fn increment_nonce(&self, address: &Address) {
        let mut account = self.get_account(address);
        account.nonce += 1;
        self.set_account(address.clone(), account);
    }
    
    /// Get contract code
    pub fn get_code(&self, address: &Address) -> Vec<u8> {
        let account = self.get_account(address);
        if account.code_hash == Hash::ZERO {
            return Vec::new();
        }
        self.code.get(&account.code_hash)
            .map(|c| c.clone())
            .unwrap_or_default()
    }
    
    /// Set contract code
    pub fn set_code(&self, address: &Address, code: Vec<u8>) {
        let code_hash = Hash::new(Keccak256::digest(&code).into());
        self.code.insert(code_hash.clone(), code);
        
        let mut account = self.get_account(address);
        account.code_hash = code_hash;
        self.set_account(address.clone(), account);
    }
    
    /// Get code hash
    pub fn get_code_hash(&self, address: &Address) -> Hash {
        self.get_account(address).code_hash
    }
    
    /// Get code size
    pub fn get_code_size(&self, address: &Address) -> usize {
        self.get_code(address).len()
    }
    
    /// Get storage value
    pub fn get_storage(&self, address: &Address, key: &U256) -> U256 {
        self.storage.get(address)
            .and_then(|s| s.get(key).copied())
            .unwrap_or(U256::ZERO)
    }
    
    /// Set storage value
    pub fn set_storage(&self, address: &Address, key: U256, value: U256) {
        let mut storage = self.storage.entry(address.clone())
            .or_insert_with(HashMap::new);
        storage.insert(key.clone(), value);
        drop(storage);
        
        self.dirty_storage.entry(address.clone())
            .or_insert_with(Vec::new)
            .push(key);
    }
    
    /// Check if account exists
    pub fn account_exists(&self, address: &Address) -> bool {
        self.accounts.contains_key(address)
    }
    
    /// Delete account (SELFDESTRUCT)
    pub fn delete_account(&self, address: &Address) {
        self.accounts.remove(address);
        self.storage.remove(address);
        self.dirty_accounts.insert(address.clone(), ());
    }
    
    /// Get current state root
    pub fn state_root(&self) -> Hash {
        self.state_root.read().clone()
    }
    
    /// Commit changes and compute new state root
    pub fn commit(&self) -> Hash {
        // In production, this would compute a Merkle Patricia Trie root
        // For now, we compute a simple hash of all account states
        let mut hasher = Keccak256::new();
        
        for entry in self.accounts.iter() {
            hasher.update(entry.key().as_bytes());
            hasher.update(&entry.value().nonce.to_le_bytes());
            hasher.update(entry.value().balance.to_le_bytes());
            hasher.update(entry.value().code_hash.as_bytes());
        }
        
        let root = Hash::new(hasher.finalize().into());
        *self.state_root.write() = root.clone();
        
        // Clear dirty tracking
        self.dirty_accounts.clear();
        self.dirty_storage.clear();
        
        root
    }
    
    /// Create snapshot for rollback
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            accounts: self.accounts.iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect(),
            storage: self.storage.iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect(),
        }
    }
    
    /// Rollback to snapshot
    pub fn rollback(&self, snapshot: StateSnapshot) {
        self.accounts.clear();
        for (addr, account) in snapshot.accounts {
            self.accounts.insert(addr, account);
        }
        
        self.storage.clear();
        for (addr, storage) in snapshot.storage {
            self.storage.insert(addr, storage);
        }
    }
}

impl Default for StateDb {
    fn default() -> Self {
        Self::new()
    }
}

/// State snapshot for transaction rollback
#[derive(Clone)]
pub struct StateSnapshot {
    accounts: HashMap<Address, Account>,
    storage: HashMap<Address, HashMap<U256, U256>>,
}

/// Journal entry for state changes (for revert)
#[derive(Clone, Debug)]
pub enum JournalEntry {
    /// Account created
    AccountCreated { address: Address },
    /// Account destroyed
    AccountDestroyed { address: Address, account: Account },
    /// Balance changed
    BalanceChanged { address: Address, prev_balance: U256 },
    /// Nonce changed
    NonceChanged { address: Address, prev_nonce: u64 },
    /// Code set
    CodeSet { address: Address, prev_code_hash: Hash },
    /// Storage changed
    StorageChanged { address: Address, key: U256, prev_value: U256 },
}

/// Journaled state for transaction execution
pub struct JournaledState {
    state: Arc<StateDb>,
    journal: Vec<JournalEntry>,
    checkpoints: Vec<usize>,
}

impl JournaledState {
    pub fn new(state: Arc<StateDb>) -> Self {
        Self {
            state,
            journal: Vec::new(),
            checkpoints: Vec::new(),
        }
    }
    
    /// Create checkpoint
    pub fn checkpoint(&mut self) -> usize {
        let id = self.checkpoints.len();
        self.checkpoints.push(self.journal.len());
        id
    }
    
    /// Commit checkpoint (discard journal entries since checkpoint)
    pub fn commit_checkpoint(&mut self, id: usize) {
        if id < self.checkpoints.len() {
            self.checkpoints.truncate(id);
        }
    }
    
    /// Revert to checkpoint
    pub fn revert_checkpoint(&mut self, id: usize) {
        if let Some(&journal_len) = self.checkpoints.get(id) {
            while self.journal.len() > journal_len {
                if let Some(entry) = self.journal.pop() {
                    self.revert_entry(entry);
                }
            }
            self.checkpoints.truncate(id);
        }
    }
    
    /// Revert a single journal entry
    fn revert_entry(&self, entry: JournalEntry) {
        match entry {
            JournalEntry::AccountCreated { address } => {
                self.state.delete_account(&address);
            }
            JournalEntry::AccountDestroyed { address, account } => {
                self.state.set_account(address, account);
            }
            JournalEntry::BalanceChanged { address, prev_balance } => {
                self.state.set_balance(&address, prev_balance);
            }
            JournalEntry::NonceChanged { address, prev_nonce } => {
                let mut account = self.state.get_account(&address);
                account.nonce = prev_nonce;
                self.state.set_account(address, account);
            }
            JournalEntry::CodeSet { address, prev_code_hash } => {
                let mut account = self.state.get_account(&address);
                account.code_hash = prev_code_hash;
                self.state.set_account(address, account);
            }
            JournalEntry::StorageChanged { address, key, prev_value } => {
                self.state.set_storage(&address, key, prev_value);
            }
        }
    }
    
    /// Set balance with journaling
    pub fn set_balance(&mut self, address: &Address, balance: U256) {
        let prev_balance = self.state.get_balance(address);
        self.journal.push(JournalEntry::BalanceChanged {
            address: address.clone(),
            prev_balance,
        });
        self.state.set_balance(address, balance);
    }
    
    /// Set storage with journaling
    pub fn set_storage(&mut self, address: &Address, key: U256, value: U256) {
        let prev_value = self.state.get_storage(address, &key);
        self.journal.push(JournalEntry::StorageChanged {
            address: address.clone(),
            key: key.clone(),
            prev_value,
        });
        self.state.set_storage(address, key, value);
    }
    
    /// Get underlying state
    pub fn state(&self) -> &Arc<StateDb> {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_account_operations() {
        let state = StateDb::new();
        let addr = Address::from([1u8; 20]);
        
        // Initial state
        assert!(state.get_account(&addr).is_empty());
        assert_eq!(state.get_balance(&addr), U256::ZERO);
        
        // Set balance
        state.set_balance(&addr, U256::from(1000));
        assert_eq!(state.get_balance(&addr), U256::from(1000));
        
        // Add balance
        state.add_balance(&addr, U256::from(500)).unwrap();
        assert_eq!(state.get_balance(&addr), U256::from(1500));
        
        // Sub balance
        state.sub_balance(&addr, U256::from(200)).unwrap();
        assert_eq!(state.get_balance(&addr), U256::from(1300));
    }
    
    #[test]
    fn test_storage_operations() {
        let state = StateDb::new();
        let addr = Address::from([1u8; 20]);
        let key = U256::from(42);
        
        // Initial storage
        assert_eq!(state.get_storage(&addr, &key), U256::ZERO);
        
        // Set storage
        state.set_storage(&addr, key.clone(), U256::from(123));
        assert_eq!(state.get_storage(&addr, &key), U256::from(123));
    }
    
    #[test]
    fn test_journaled_state_revert() {
        let state = Arc::new(StateDb::new());
        let mut journal = JournaledState::new(state.clone());
        let addr = Address::from([1u8; 20]);
        
        // Initial balance
        state.set_balance(&addr, U256::from(1000));
        
        // Checkpoint
        let cp = journal.checkpoint();
        
        // Modify
        journal.set_balance(&addr, U256::from(500));
        assert_eq!(state.get_balance(&addr), U256::from(500));
        
        // Revert
        journal.revert_checkpoint(cp);
        assert_eq!(state.get_balance(&addr), U256::from(1000));
    }
}
