//! Storage layer

use std::sync::Arc;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use nexus_primitives::{Address, Hash, U256};
use crate::{StateResult, StateError, MerkleTrie};

/// Account state
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountState {
    /// Account nonce
    pub nonce: u64,
    /// Account balance
    pub balance: U256,
    /// Code hash (for contracts)
    pub code_hash: Hash,
    /// Storage root
    pub storage_root: Hash,
}

/// State database
pub struct StateDb {
    /// Account states
    accounts: DashMap<Address, AccountState>,
    /// Account code
    code: DashMap<Hash, Vec<u8>>,
    /// Account storage
    storage: DashMap<(Address, U256), U256>,
    /// State trie
    trie: RwLock<MerkleTrie>,
}

impl StateDb {
    /// Create new state database
    pub fn new() -> Self {
        Self {
            accounts: DashMap::new(),
            code: DashMap::new(),
            storage: DashMap::new(),
            trie: RwLock::new(MerkleTrie::new()),
        }
    }
    
    /// Get state root
    pub fn state_root(&self) -> Hash {
        self.trie.read().root().clone()
    }
    
    /// Get account
    pub fn get_account(&self, address: &Address) -> AccountState {
        self.accounts.get(address)
            .map(|a| a.clone())
            .unwrap_or_default()
    }
    
    /// Set account
    pub fn set_account(&self, address: &Address, account: AccountState) {
        self.accounts.insert(address.clone(), account);
    }
    
    /// Get balance
    pub fn get_balance(&self, address: &Address) -> U256 {
        self.get_account(address).balance
    }
    
    /// Set balance
    pub fn set_balance(&self, address: &Address, balance: U256) {
        let mut account = self.get_account(address);
        account.balance = balance;
        self.set_account(address, account);
    }
    
    /// Add balance
    pub fn add_balance(&self, address: &Address, amount: U256) {
        let mut account = self.get_account(address);
        account.balance = account.balance + amount;
        self.set_account(address, account);
    }
    
    /// Subtract balance
    pub fn sub_balance(&self, address: &Address, amount: U256) -> StateResult<()> {
        let mut account = self.get_account(address);
        if account.balance < amount {
            return Err(StateError::Storage("Insufficient balance".into()));
        }
        account.balance = account.balance - amount;
        self.set_account(address, account);
        Ok(())
    }
    
    /// Get nonce
    pub fn get_nonce(&self, address: &Address) -> u64 {
        self.get_account(address).nonce
    }
    
    /// Set nonce
    pub fn set_nonce(&self, address: &Address, nonce: u64) {
        let mut account = self.get_account(address);
        account.nonce = nonce;
        self.set_account(address, account);
    }
    
    /// Increment nonce
    pub fn increment_nonce(&self, address: &Address) {
        let mut account = self.get_account(address);
        account.nonce += 1;
        self.set_account(address, account);
    }
    
    /// Get code hash
    pub fn get_code_hash(&self, address: &Address) -> Hash {
        self.get_account(address).code_hash
    }
    
    /// Get code
    pub fn get_code(&self, address: &Address) -> Vec<u8> {
        let account = self.get_account(address);
        self.code.get(&account.code_hash)
            .map(|c| c.clone())
            .unwrap_or_default()
    }
    
    /// Set code
    pub fn set_code(&self, address: &Address, code: Vec<u8>) {
        use sha3::{Digest, Keccak256};
        
        let code_hash = Hash::new(Keccak256::digest(&code).as_slice().try_into().unwrap());
        self.code.insert(code_hash.clone(), code);
        
        let mut account = self.get_account(address);
        account.code_hash = code_hash;
        self.set_account(address, account);
    }
    
    /// Get storage value
    pub fn get_storage(&self, address: &Address, key: &U256) -> U256 {
        self.storage.get(&(address.clone(), key.clone()))
            .map(|v| v.clone())
            .unwrap_or(U256::ZERO)
    }
    
    /// Set storage value
    pub fn set_storage(&self, address: &Address, key: U256, value: U256) {
        if value == U256::ZERO {
            self.storage.remove(&(address.clone(), key));
        } else {
            self.storage.insert((address.clone(), key), value);
        }
    }
    
    /// Check if account exists
    pub fn account_exists(&self, address: &Address) -> bool {
        self.accounts.contains_key(address)
    }
    
    /// Check if account is empty
    pub fn is_empty(&self, address: &Address) -> bool {
        let account = self.get_account(address);
        account.nonce == 0 && 
        account.balance == U256::ZERO && 
        account.code_hash == Hash::ZERO
    }
    
    /// Delete account
    pub fn delete_account(&self, address: &Address) {
        self.accounts.remove(address);
        
        // Remove storage
        self.storage.retain(|(addr, _), _| addr != address);
    }
    
    /// Commit changes to trie
    pub fn commit(&self) -> Hash {
        let mut trie = self.trie.write();
        
        for entry in self.accounts.iter() {
            let address = entry.key();
            let account = entry.value();
            
            let encoded = serde_json::to_vec(account).unwrap_or_default();
            trie.insert(address.as_bytes(), encoded).ok();
        }
        
        trie.root().clone()
    }
    
    /// Get account count
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }
}

impl Default for StateDb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_state_db() {
        let db = StateDb::new();
        let addr = Address::from([1u8; 20]);
        
        // Test balance
        db.set_balance(&addr, U256::from(100));
        assert_eq!(db.get_balance(&addr), U256::from(100));
        
        db.add_balance(&addr, U256::from(50));
        assert_eq!(db.get_balance(&addr), U256::from(150));
        
        db.sub_balance(&addr, U256::from(30)).unwrap();
        assert_eq!(db.get_balance(&addr), U256::from(120));
        
        // Test nonce
        db.increment_nonce(&addr);
        assert_eq!(db.get_nonce(&addr), 1);
        
        // Test code
        db.set_code(&addr, vec![0x60, 0x80, 0x60, 0x40]);
        assert_eq!(db.get_code(&addr), vec![0x60, 0x80, 0x60, 0x40]);
        
        // Test storage
        db.set_storage(&addr, U256::from(0), U256::from(42));
        assert_eq!(db.get_storage(&addr, &U256::from(0)), U256::from(42));
    }
}
