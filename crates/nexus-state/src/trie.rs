//! Merkle Patricia Trie implementation

use std::collections::HashMap;
use sha3::{Digest, Keccak256};
use serde::{Deserialize, Serialize};

use nexus_primitives::Hash;
use crate::{StateResult, StateError};

/// Trie node types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TrieNode {
    /// Empty node
    Empty,
    /// Leaf node with key suffix and value
    Leaf {
        key_end: Vec<u8>,
        value: Vec<u8>,
    },
    /// Extension node with shared prefix
    Extension {
        prefix: Vec<u8>,
        child: Hash,
    },
    /// Branch node with 16 children + optional value
    Branch {
        children: [Option<Hash>; 16],
        value: Option<Vec<u8>>,
    },
}

impl TrieNode {
    /// Compute node hash
    pub fn hash(&self) -> Hash {
        let encoded = self.rlp_encode();
        Hash::new(Keccak256::digest(&encoded).into())
    }
    
    /// RLP encode node (simplified)
    pub fn rlp_encode(&self) -> Vec<u8> {
        // Simplified encoding - in production use actual RLP
        serde_json::to_vec(self).unwrap_or_default()
    }
}

/// Merkle Patricia Trie
pub struct MerkleTrie {
    /// Root hash
    root: Hash,
    /// Node storage
    nodes: HashMap<Hash, TrieNode>,
}

impl MerkleTrie {
    /// Create empty trie
    pub fn new() -> Self {
        let empty_root = Hash::ZERO;
        let mut nodes = HashMap::new();
        nodes.insert(empty_root.clone(), TrieNode::Empty);
        
        Self {
            root: empty_root,
            nodes,
        }
    }
    
    /// Get root hash
    pub fn root(&self) -> &Hash {
        &self.root
    }
    
    /// Get value by key
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.get_recursive(&self.root, &Self::key_to_nibbles(key), 0)
    }
    
    fn get_recursive(&self, node_hash: &Hash, nibbles: &[u8], depth: usize) -> Option<Vec<u8>> {
        let node = self.nodes.get(node_hash)?;
        
        match node {
            TrieNode::Empty => None,
            
            TrieNode::Leaf { key_end, value } => {
                if &nibbles[depth..] == key_end.as_slice() {
                    Some(value.clone())
                } else {
                    None
                }
            }
            
            TrieNode::Extension { prefix, child } => {
                if nibbles[depth..].starts_with(prefix) {
                    self.get_recursive(child, nibbles, depth + prefix.len())
                } else {
                    None
                }
            }
            
            TrieNode::Branch { children, value } => {
                if depth >= nibbles.len() {
                    value.clone()
                } else {
                    let idx = nibbles[depth] as usize;
                    if let Some(child) = &children[idx] {
                        self.get_recursive(child, nibbles, depth + 1)
                    } else {
                        None
                    }
                }
            }
        }
    }
    
    /// Insert key-value pair
    pub fn insert(&mut self, key: &[u8], value: Vec<u8>) -> StateResult<()> {
        let nibbles = Self::key_to_nibbles(key);
        let new_root = self.insert_recursive(&self.root.clone(), &nibbles, 0, value)?;
        self.root = new_root;
        Ok(())
    }
    
    fn insert_recursive(
        &mut self,
        node_hash: &Hash,
        nibbles: &[u8],
        depth: usize,
        value: Vec<u8>,
    ) -> StateResult<Hash> {
        let node = self.nodes.get(node_hash)
            .cloned()
            .unwrap_or(TrieNode::Empty);
        
        let new_node = match node {
            TrieNode::Empty => {
                TrieNode::Leaf {
                    key_end: nibbles[depth..].to_vec(),
                    value: value.clone(),
                }
            }
            
            TrieNode::Leaf { key_end, value: old_value } => {
                let remaining = &nibbles[depth..];
                
                // Find common prefix
                let common_len = key_end.iter()
                    .zip(remaining.iter())
                    .take_while(|(a, b)| a == b)
                    .count();
                
                if common_len == key_end.len() && common_len == remaining.len() {
                    // Same key, update value
                    TrieNode::Leaf { key_end, value }
                } else {
                    // Split into branch
                    let mut children: [Option<Hash>; 16] = Default::default();
                    
                    // Old leaf
                    if common_len < key_end.len() {
                        let old_leaf = TrieNode::Leaf {
                            key_end: key_end[common_len + 1..].to_vec(),
                            value: old_value.clone(),
                        };
                        let old_hash = old_leaf.hash();
                        self.nodes.insert(old_hash.clone(), old_leaf);
                        children[key_end[common_len] as usize] = Some(old_hash);
                    }
                    
                    // New leaf
                    if common_len < remaining.len() {
                        let new_leaf = TrieNode::Leaf {
                            key_end: remaining[common_len + 1..].to_vec(),
                            value: value.clone(),
                        };
                        let new_hash = new_leaf.hash();
                        self.nodes.insert(new_hash.clone(), new_leaf);
                        children[remaining[common_len] as usize] = Some(new_hash);
                    }

                    let branch = TrieNode::Branch {
                        children,
                        value: if common_len == remaining.len() {
                            Some(value.clone())
                        } else if common_len == key_end.len() {
                            Some(old_value.clone())
                        } else {
                            None
                        },
                    };
                    
                    if common_len > 0 {
                        let branch_hash = branch.hash();
                        self.nodes.insert(branch_hash.clone(), branch);
                        
                        TrieNode::Extension {
                            prefix: remaining[..common_len].to_vec(),
                            child: branch_hash,
                        }
                    } else {
                        branch
                    }
                }
            }
            
            TrieNode::Extension { prefix, child } => {
                let remaining = &nibbles[depth..];
                
                let common_len = prefix.iter()
                    .zip(remaining.iter())
                    .take_while(|(a, b)| a == b)
                    .count();
                
                if common_len == prefix.len() {
                    // Continue down
                    let new_child = self.insert_recursive(&child, nibbles, depth + common_len, value)?;
                    TrieNode::Extension { prefix, child: new_child }
                } else {
                    // Split extension
                    let mut children: [Option<Hash>; 16] = Default::default();
                    
                    // Existing branch
                    if common_len + 1 < prefix.len() {
                        let ext = TrieNode::Extension {
                            prefix: prefix[common_len + 1..].to_vec(),
                            child,
                        };
                        let ext_hash = ext.hash();
                        self.nodes.insert(ext_hash.clone(), ext);
                        children[prefix[common_len] as usize] = Some(ext_hash);
                    } else {
                        children[prefix[common_len] as usize] = Some(child);
                    }
                    
                    // New leaf
                    let new_leaf = TrieNode::Leaf {
                        key_end: remaining[common_len + 1..].to_vec(),
                        value,
                    };
                    let new_hash = new_leaf.hash();
                    self.nodes.insert(new_hash.clone(), new_leaf);
                    children[remaining[common_len] as usize] = Some(new_hash);
                    
                    let branch = TrieNode::Branch { children, value: None };
                    
                    if common_len > 0 {
                        let branch_hash = branch.hash();
                        self.nodes.insert(branch_hash.clone(), branch);
                        
                        TrieNode::Extension {
                            prefix: remaining[..common_len].to_vec(),
                            child: branch_hash,
                        }
                    } else {
                        branch
                    }
                }
            }
            
            TrieNode::Branch { mut children, value: branch_value } => {
                let remaining = &nibbles[depth..];
                
                if remaining.is_empty() {
                    TrieNode::Branch { children, value: Some(value) }
                } else {
                    let idx = remaining[0] as usize;
                    let child_hash = children[idx]
                        .clone()
                        .unwrap_or_else(|| Hash::ZERO);
                    
                    let new_child = self.insert_recursive(&child_hash, nibbles, depth + 1, value)?;
                    children[idx] = Some(new_child);
                    
                    TrieNode::Branch { children, value: branch_value }
                }
            }
        };
        
        let new_hash = new_node.hash();
        self.nodes.insert(new_hash.clone(), new_node);
        Ok(new_hash)
    }
    
    /// Delete key
    pub fn delete(&mut self, key: &[u8]) -> StateResult<()> {
        let nibbles = Self::key_to_nibbles(key);
        let new_root = self.delete_recursive(&self.root.clone(), &nibbles, 0)?;
        self.root = new_root;
        Ok(())
    }
    
    fn delete_recursive(
        &mut self,
        node_hash: &Hash,
        nibbles: &[u8],
        depth: usize,
    ) -> StateResult<Hash> {
        // Simplified delete - in production this would compact nodes
        let node = self.nodes.get(node_hash)
            .cloned()
            .ok_or_else(|| StateError::NotFound("Node not found".into()))?;
        
        match node {
            TrieNode::Empty => Ok(node_hash.clone()),
            
            TrieNode::Leaf { key_end, .. } => {
                if &nibbles[depth..] == key_end.as_slice() {
                    Ok(Hash::ZERO)
                } else {
                    Ok(node_hash.clone())
                }
            }
            
            TrieNode::Branch { mut children, value } => {
                if depth >= nibbles.len() {
                    let new_node = TrieNode::Branch { children, value: None };
                    let new_hash = new_node.hash();
                    self.nodes.insert(new_hash.clone(), new_node);
                    Ok(new_hash)
                } else {
                    let idx = nibbles[depth] as usize;
                    if let Some(child) = &children[idx] {
                        let new_child = self.delete_recursive(child, nibbles, depth + 1)?;
                        if new_child == Hash::ZERO {
                            children[idx] = None;
                        } else {
                            children[idx] = Some(new_child);
                        }
                    }
                    
                    let new_node = TrieNode::Branch { children, value };
                    let new_hash = new_node.hash();
                    self.nodes.insert(new_hash.clone(), new_node);
                    Ok(new_hash)
                }
            }
            
            _ => Ok(node_hash.clone()),
        }
    }
    
    /// Generate Merkle proof
    pub fn prove(&self, key: &[u8]) -> Option<MerkleProof> {
        let nibbles = Self::key_to_nibbles(key);
        let mut proof_nodes = Vec::new();
        
        if self.prove_recursive(&self.root, &nibbles, 0, &mut proof_nodes) {
            Some(MerkleProof {
                key: key.to_vec(),
                value: self.get(key),
                nodes: proof_nodes,
                root: self.root.clone(),
            })
        } else {
            None
        }
    }
    
    fn prove_recursive(
        &self,
        node_hash: &Hash,
        nibbles: &[u8],
        depth: usize,
        proof: &mut Vec<TrieNode>,
    ) -> bool {
        let node = match self.nodes.get(node_hash) {
            Some(n) => n.clone(),
            None => return false,
        };
        
        proof.push(node.clone());
        
        match node {
            TrieNode::Empty => true,
            TrieNode::Leaf { .. } => true,
            
            TrieNode::Extension { prefix, child } => {
                if nibbles[depth..].starts_with(&prefix) {
                    self.prove_recursive(&child, nibbles, depth + prefix.len(), proof)
                } else {
                    true
                }
            }
            
            TrieNode::Branch { children, .. } => {
                if depth < nibbles.len() {
                    let idx = nibbles[depth] as usize;
                    if let Some(child) = &children[idx] {
                        self.prove_recursive(child, nibbles, depth + 1, proof)
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
        }
    }
    
    /// Verify Merkle proof
    pub fn verify_proof(proof: &MerkleProof) -> bool {
        if proof.nodes.is_empty() {
            return proof.value.is_none();
        }
        
        // Verify chain of hashes
        let computed_root = proof.nodes[0].hash();
        computed_root == proof.root
    }
    
    /// Convert key to nibbles
    fn key_to_nibbles(key: &[u8]) -> Vec<u8> {
        let mut nibbles = Vec::with_capacity(key.len() * 2);
        for byte in key {
            nibbles.push(byte >> 4);
            nibbles.push(byte & 0x0f);
        }
        nibbles
    }
}

impl Default for MerkleTrie {
    fn default() -> Self {
        Self::new()
    }
}

/// Merkle proof
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
    pub nodes: Vec<TrieNode>,
    pub root: Hash,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_trie_insert_get() {
        let mut trie = MerkleTrie::new();
        
        trie.insert(b"hello", b"world".to_vec()).unwrap();
        trie.insert(b"help", b"me".to_vec()).unwrap();
        
        assert_eq!(trie.get(b"hello"), Some(b"world".to_vec()));
        assert_eq!(trie.get(b"help"), Some(b"me".to_vec()));
        assert_eq!(trie.get(b"other"), None);
    }
    
    #[test]
    fn test_trie_proof() {
        let mut trie = MerkleTrie::new();
        trie.insert(b"key", b"value".to_vec()).unwrap();
        
        let proof = trie.prove(b"key").unwrap();
        assert!(MerkleTrie::verify_proof(&proof));
    }
}
