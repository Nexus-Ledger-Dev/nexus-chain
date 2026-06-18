//! Pending transaction pool.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use nexus_primitives::{Hash, Transaction};

/// In-process mempool holding transactions not yet included in a DAG vertex.
pub struct Mempool {
    pending: Mutex<VecDeque<Transaction>>,
    by_hash: Mutex<HashMap<Hash, Transaction>>,
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            by_hash: Mutex::new(HashMap::new()),
            max_size,
        }
    }

    /// Add a transaction. Returns false if the pool is full or the tx is a duplicate.
    pub fn add(&self, tx: Transaction) -> bool {
        let hash = tx.hash();
        let mut by_hash = self.by_hash.lock().unwrap();
        if by_hash.contains_key(&hash) {
            return false;
        }
        let mut pending = self.pending.lock().unwrap();
        if pending.len() >= self.max_size {
            return false;
        }
        by_hash.insert(hash, tx.clone());
        pending.push_back(tx);
        true
    }

    /// Look up transactions by hash (used by sync layer).
    pub fn get_by_hashes(&self, hashes: &[Hash]) -> Vec<Transaction> {
        let by_hash = self.by_hash.lock().unwrap();
        hashes.iter().filter_map(|h| by_hash.get(h).cloned()).collect()
    }

    /// Pull up to `limit` transactions for inclusion in a vertex.
    pub fn drain_batch(&self, limit: usize) -> Vec<Transaction> {
        let mut pending = self.pending.lock().unwrap();
        let mut by_hash = self.by_hash.lock().unwrap();
        let len = pending.len();
        let batch: Vec<Transaction> = pending.drain(..len.min(limit)).collect();
        for tx in &batch {
            by_hash.remove(&tx.hash());
        }
        batch
    }

    pub fn len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
