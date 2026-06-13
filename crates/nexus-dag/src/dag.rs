//! Core DAG data structure and operations

use crate::{Vertex, VertexError, VertexHeader, VertexStatus};
use dashmap::DashMap;
use nexus_primitives::{Hash, Height, NexusError, Result, Timestamp, ValidatorId};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Configuration for DAG behavior
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagConfig {
    /// Maximum depth for tip selection
    pub max_tip_depth: Height,
    
    /// Minimum weight threshold for confirmation
    pub confirmation_threshold: u128,
    
    /// Maximum vertices to keep in memory
    pub max_vertices_in_memory: usize,
    
    /// Pruning depth (finalized vertices below this depth can be pruned)
    pub pruning_depth: Height,
    
    /// Maximum time drift allowed for vertex timestamps (seconds)
    pub max_time_drift: u64,
}

impl Default for DagConfig {
    fn default() -> Self {
        Self {
            max_tip_depth: 100,
            confirmation_threshold: 1000,
            max_vertices_in_memory: 100_000,
            pruning_depth: 10_000,
            max_time_drift: 30,
        }
    }
}

/// The main DAG structure
pub struct Dag {
    /// Configuration
    config: DagConfig,
    
    /// All vertices (hash -> vertex)
    vertices: DashMap<Hash, Arc<Vertex>>,
    
    /// Vertex status tracking
    status: DashMap<Hash, VertexStatus>,
    
    /// Children mapping (parent hash -> children hashes)
    children: DashMap<Hash, HashSet<Hash>>,
    
    /// Current tips (vertices with no children)
    tips: RwLock<HashSet<Hash>>,
    
    /// Vertices by height for efficient queries
    by_height: DashMap<Height, HashSet<Hash>>,
    
    /// Latest finalized height
    finalized_height: RwLock<Height>,
    
    /// Genesis hash
    genesis_hash: RwLock<Option<Hash>>,
    
    /// Pending vertices (awaiting parents)
    pending: DashMap<Hash, Vertex>,
    
    /// Orphan dependencies (missing parent -> waiting vertices)
    orphan_deps: DashMap<Hash, HashSet<Hash>>,
}

impl Dag {
    /// Create a new DAG with configuration
    pub fn new(config: DagConfig) -> Self {
        Self {
            config,
            vertices: DashMap::new(),
            status: DashMap::new(),
            children: DashMap::new(),
            tips: RwLock::new(HashSet::new()),
            by_height: DashMap::new(),
            finalized_height: RwLock::new(0),
            genesis_hash: RwLock::new(None),
            pending: DashMap::new(),
            orphan_deps: DashMap::new(),
        }
    }
    
    /// Initialize DAG with genesis vertex
    pub fn init_genesis(&self, genesis: Vertex) -> Result<()> {
        if self.genesis_hash.read().is_some() {
            return Err(NexusError::Dag("Genesis already set".into()));
        }
        
        let hash = genesis.hash;
        
        // Store genesis
        self.vertices.insert(hash, Arc::new(genesis));
        self.status.insert(hash, VertexStatus::Finalized);
        self.by_height.entry(0).or_default().insert(hash);
        self.tips.write().insert(hash);
        
        *self.genesis_hash.write() = Some(hash);
        
        info!("DAG initialized with genesis: {:?}", hash);
        Ok(())
    }
    
    /// Add a vertex to the DAG
    pub fn add_vertex(&self, vertex: Vertex) -> Result<()> {
        let hash = vertex.hash;
        
        // Check if already exists
        if self.vertices.contains_key(&hash) {
            return Ok(()); // Idempotent
        }
        
        // Verify structure
        vertex.verify_structure()
            .map_err(|e| NexusError::Dag(e.to_string()))?;
        
        // Check timestamp
        self.verify_timestamp(&vertex)?;
        
        // Check if all parents exist
        let missing_parents: Vec<Hash> = vertex.parents.iter()
            .filter(|p| !self.vertices.contains_key(*p))
            .cloned()
            .collect();
        
        if !missing_parents.is_empty() {
            // Add to pending
            debug!("Vertex {:?} pending, missing parents: {:?}", hash, missing_parents);
            
            for parent in &missing_parents {
                self.orphan_deps.entry(*parent).or_default().insert(hash);
            }
            
            self.pending.insert(hash, vertex);
            self.status.insert(hash, VertexStatus::AwaitingParents);
            
            return Ok(());
        }
        
        // All parents present, add to DAG
        self.insert_vertex(vertex)
    }
    
    /// Internal vertex insertion (parents verified)
    fn insert_vertex(&self, vertex: Vertex) -> Result<()> {
        let hash = vertex.hash;
        let height = vertex.height;
        let parents = vertex.parents.clone();
        
        // Verify height is correct
        let expected_height = parents.iter()
            .filter_map(|p| self.vertices.get(p).map(|v| v.height))
            .max()
            .map(|h| h + 1)
            .unwrap_or(0);
        
        if vertex.height != expected_height {
            return Err(NexusError::Dag(format!(
                "Invalid height: expected {}, got {}",
                expected_height, vertex.height
            )));
        }
        
        // Store vertex
        let vertex = Arc::new(vertex);
        self.vertices.insert(hash, vertex);
        self.status.insert(hash, VertexStatus::Confirmed);
        
        // Update children mappings
        for parent in &parents {
            self.children.entry(*parent).or_default().insert(hash);
            
            // Parent is no longer a tip
            self.tips.write().remove(parent);
        }
        
        // This vertex is now a tip
        self.tips.write().insert(hash);
        
        // Add to height index
        self.by_height.entry(height).or_default().insert(hash);
        
        debug!("Vertex {:?} added at height {}", hash, height);
        
        // Process any pending vertices that were waiting for this one
        self.process_orphans(&hash)?;
        
        Ok(())
    }
    
    /// Process pending vertices that were waiting for a parent
    fn process_orphans(&self, new_parent: &Hash) -> Result<()> {
        let waiting: Vec<Hash> = self.orphan_deps
            .remove(new_parent)
            .map(|(_, set)| set.into_iter().collect())
            .unwrap_or_default();
        
        for orphan_hash in waiting {
            if let Some((_, vertex)) = self.pending.remove(&orphan_hash) {
                // Check if all parents now exist
                let still_missing: Vec<Hash> = vertex.parents.iter()
                    .filter(|p| !self.vertices.contains_key(*p))
                    .cloned()
                    .collect();
                
                if still_missing.is_empty() {
                    // All parents present, insert
                    self.insert_vertex(vertex)?;
                } else {
                    // Still missing some parents
                    for parent in &still_missing {
                        self.orphan_deps.entry(*parent).or_default().insert(orphan_hash);
                    }
                    self.pending.insert(orphan_hash, vertex);
                }
            }
        }
        
        Ok(())
    }
    
    /// Verify vertex timestamp
    fn verify_timestamp(&self, vertex: &Vertex) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let max_future = now + self.config.max_time_drift * 1000;
        
        if vertex.timestamp > max_future {
            return Err(NexusError::Dag("Vertex timestamp too far in future".into()));
        }
        
        // Check against parent timestamps
        for parent_hash in &vertex.parents {
            if let Some(parent) = self.vertices.get(parent_hash) {
                if vertex.timestamp < parent.timestamp {
                    return Err(NexusError::Dag("Vertex timestamp before parent".into()));
                }
            }
        }
        
        Ok(())
    }
    
    /// Get current tips
    pub fn get_tips(&self) -> Vec<Hash> {
        self.tips.read().iter().cloned().collect()
    }
    
    /// Get a vertex by hash
    pub fn get_vertex(&self, hash: &Hash) -> Option<Arc<Vertex>> {
        self.vertices.get(hash).map(|v| Arc::clone(&v))
    }
    
    /// Get vertex status
    pub fn get_status(&self, hash: &Hash) -> Option<VertexStatus> {
        self.status.get(hash).map(|s| *s)
    }
    
    /// Get vertices at a specific height
    pub fn get_vertices_at_height(&self, height: Height) -> Vec<Hash> {
        self.by_height
            .get(&height)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    /// Get vertex children
    pub fn get_children(&self, hash: &Hash) -> Vec<Hash> {
        self.children
            .get(hash)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    /// Get latest confirmed height
    pub fn latest_height(&self) -> Height {
        self.tips.read()
            .iter()
            .filter_map(|h| self.vertices.get(h).map(|v| v.height))
            .max()
            .unwrap_or(0)
    }
    
    /// Get finalized height
    pub fn finalized_height(&self) -> Height {
        *self.finalized_height.read()
    }
    
    /// Set finalized height (called by finality module)
    pub fn set_finalized_height(&self, height: Height) {
        let mut finalized = self.finalized_height.write();
        if height > *finalized {
            *finalized = height;
            
            // Update status for finalized vertices
            if let Some(hashes) = self.by_height.get(&height) {
                for hash in hashes.iter() {
                    self.status.insert(*hash, VertexStatus::Finalized);
                }
            }
            
            info!("Finalized height updated to {}", height);
        }
    }
    
    /// Calculate weight for a vertex (based on descendants)
    pub fn calculate_weight(&self, hash: &Hash) -> u128 {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*hash);
        
        let mut weight = 0u128;
        
        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            
            if let Some(vertex) = self.vertices.get(&current) {
                weight += 1;
                
                for child in self.get_children(&current) {
                    if !visited.contains(&child) {
                        queue.push_back(child);
                    }
                }
            }
        }
        
        weight
    }
    
    /// Get ancestors up to a certain depth
    pub fn get_ancestors(&self, hash: &Hash, max_depth: usize) -> Vec<Hash> {
        let mut ancestors = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        if let Some(vertex) = self.vertices.get(hash) {
            for parent in &vertex.parents {
                queue.push_back((*parent, 1usize));
            }
        }
        
        while let Some((current, depth)) = queue.pop_front() {
            if visited.contains(&current) || depth > max_depth {
                continue;
            }
            visited.insert(current);
            ancestors.push(current);
            
            if let Some(vertex) = self.vertices.get(&current) {
                for parent in &vertex.parents {
                    if !visited.contains(parent) {
                        queue.push_back((*parent, depth + 1));
                    }
                }
            }
        }
        
        ancestors
    }
    
    /// Check if vertex A is an ancestor of vertex B
    pub fn is_ancestor(&self, ancestor: &Hash, descendant: &Hash) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*descendant);
        
        while let Some(current) = queue.pop_front() {
            if current == *ancestor {
                return true;
            }
            
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            
            if let Some(vertex) = self.vertices.get(&current) {
                for parent in &vertex.parents {
                    if !visited.contains(parent) {
                        queue.push_back(*parent);
                    }
                }
            }
        }
        
        false
    }
    
    /// Get DAG statistics
    pub fn stats(&self) -> DagStats {
        DagStats {
            total_vertices: self.vertices.len(),
            tips_count: self.tips.read().len(),
            pending_count: self.pending.len(),
            latest_height: self.latest_height(),
            finalized_height: self.finalized_height(),
        }
    }
    
    /// Prune old finalized vertices (memory management)
    pub fn prune(&self, keep_above_height: Height) -> usize {
        let finalized = self.finalized_height();
        
        if keep_above_height >= finalized {
            return 0; // Don't prune unfinalized
        }
        
        let mut pruned = 0;
        
        for height in 0..keep_above_height {
            if let Some((_, hashes)) = self.by_height.remove(&height) {
                for hash in hashes {
                    self.vertices.remove(&hash);
                    self.status.remove(&hash);
                    self.children.remove(&hash);
                    pruned += 1;
                }
            }
        }
        
        info!("Pruned {} vertices below height {}", pruned, keep_above_height);
        pruned
    }
}

/// DAG statistics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagStats {
    pub total_vertices: usize,
    pub tips_count: usize,
    pub pending_count: usize,
    pub latest_height: Height,
    pub finalized_height: Height,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_primitives::Timestamp;
    
    fn create_test_dag() -> Dag {
        let dag = Dag::new(DagConfig::default());
        let genesis = Vertex::genesis(1337, 0);
        dag.init_genesis(genesis).unwrap();
        dag
    }
    
    #[test]
    fn test_dag_genesis() {
        let dag = create_test_dag();
        
        assert_eq!(dag.latest_height(), 0);
        assert_eq!(dag.get_tips().len(), 1);
    }
    
    #[test]
    fn test_dag_add_vertex() {
        let dag = create_test_dag();
        let genesis_hash = dag.get_tips()[0];
        
        let vertex = Vertex::new(
            vec![genesis_hash],
            1,
            1000,
            ValidatorId([1; 32]),
            vec![],
            Hash::ZERO,
            Hash::ZERO,
        );
        
        dag.add_vertex(vertex).unwrap();
        
        assert_eq!(dag.latest_height(), 1);
        assert_eq!(dag.get_tips().len(), 1);
    }
    
    #[test]
    fn test_dag_multiple_tips() {
        let dag = create_test_dag();
        let genesis_hash = dag.get_tips()[0];
        
        // Add two vertices referencing genesis
        let vertex1 = Vertex::new(
            vec![genesis_hash],
            1,
            1000,
            ValidatorId([1; 32]),
            vec![],
            Hash::ZERO,
            Hash::ZERO,
        );
        
        let vertex2 = Vertex::new(
            vec![genesis_hash],
            1,
            1001,
            ValidatorId([2; 32]),
            vec![],
            Hash::ZERO,
            Hash::ZERO,
        );
        
        dag.add_vertex(vertex1).unwrap();
        dag.add_vertex(vertex2).unwrap();
        
        assert_eq!(dag.get_tips().len(), 2);
    }
    
    #[test]
    fn test_ancestor_check() {
        let dag = create_test_dag();
        let genesis_hash = dag.get_tips()[0];
        
        let vertex = Vertex::new(
            vec![genesis_hash],
            1,
            1000,
            ValidatorId([1; 32]),
            vec![],
            Hash::ZERO,
            Hash::ZERO,
        );
        let v1_hash = vertex.hash;
        dag.add_vertex(vertex).unwrap();
        
        assert!(dag.is_ancestor(&genesis_hash, &v1_hash));
        assert!(!dag.is_ancestor(&v1_hash, &genesis_hash));
    }
}
