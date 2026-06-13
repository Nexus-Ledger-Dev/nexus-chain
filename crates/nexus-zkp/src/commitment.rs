//! Pedersen commitments for value hiding

use ark_bn254::{Fr, G1Affine, G1Projective};
use ark_ec::{AffineRepr, CurveGroup, Group};
use ark_ff::PrimeField;
use ark_std::{rand::Rng, UniformRand};
use serde::{Deserialize, Serialize};
use crate::{ZkpError, ZkpResult};

/// Generator points for Pedersen commitments
pub struct PedersenGenerators {
    /// Generator for value
    pub g: G1Affine,
    /// Generator for blinding factor
    pub h: G1Affine,
}

impl PedersenGenerators {
    /// Create new generators (deterministic from seed)
    pub fn new() -> Self {
        // In production, these would be generated via a trusted setup
        // or using hash-to-curve for verifiable randomness
        let mut rng = ark_std::test_rng();
        
        Self {
            g: G1Projective::rand(&mut rng).into_affine(),
            h: G1Projective::rand(&mut rng).into_affine(),
        }
    }
    
    /// Create commitment: C = v*G + r*H
    pub fn commit(&self, value: &Fr, blinding: &Fr) -> G1Affine {
        let vg = self.g * value;
        let rh = self.h * blinding;
        (vg + rh).into_affine()
    }
    
    /// Verify commitment opening
    pub fn verify(&self, commitment: &G1Affine, value: &Fr, blinding: &Fr) -> bool {
        let expected = self.commit(value, blinding);
        *commitment == expected
    }
}

impl Default for PedersenGenerators {
    fn default() -> Self {
        Self::new()
    }
}

/// A Pedersen commitment
#[derive(Clone, Copy, Debug)]
pub struct Commitment {
    pub point: G1Affine,
}

impl Commitment {
    pub fn new(generators: &PedersenGenerators, value: u64, blinding: &Fr) -> Self {
        let value_fr = Fr::from(value);
        Self {
            point: generators.commit(&value_fr, blinding),
        }
    }
    
    /// Create commitment with random blinding factor
    pub fn new_random<R: Rng>(generators: &PedersenGenerators, value: u64, rng: &mut R) -> (Self, Fr) {
        let blinding = Fr::rand(rng);
        let commitment = Self::new(generators, value, &blinding);
        (commitment, blinding)
    }
    
    /// Verify the commitment opens to given value
    pub fn verify(&self, generators: &PedersenGenerators, value: u64, blinding: &Fr) -> bool {
        let value_fr = Fr::from(value);
        generators.verify(&self.point, &value_fr, blinding)
    }
    
    /// Homomorphic addition of commitments
    pub fn add(&self, other: &Commitment) -> Commitment {
        Commitment {
            point: (self.point + other.point).into_affine(),
        }
    }
    
    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        use ark_serialize::CanonicalSerialize;
        let mut bytes = [0u8; 32];
        // Compressed point serialization
        self.point.serialize_compressed(&mut bytes.as_mut_slice()).unwrap();
        bytes
    }
}

/// Note for private transactions (UTXO-style)
#[derive(Clone, Debug)]
pub struct Note {
    /// Value of the note
    pub value: u64,
    /// Commitment to the value
    pub commitment: Commitment,
    /// Blinding factor (secret)
    pub blinding: Fr,
    /// Owner's public key
    pub owner: [u8; 32],
    /// Note nullifier (for spending)
    nullifier_secret: Fr,
}

impl Note {
    /// Create a new note
    pub fn new<R: Rng>(
        generators: &PedersenGenerators,
        value: u64,
        owner: [u8; 32],
        rng: &mut R,
    ) -> Self {
        let blinding = Fr::rand(rng);
        let commitment = Commitment::new(generators, value, &blinding);
        let nullifier_secret = Fr::rand(rng);
        
        Self {
            value,
            commitment,
            blinding,
            owner,
            nullifier_secret,
        }
    }
    
    /// Calculate nullifier for this note
    pub fn nullifier(&self) -> [u8; 32] {
        use ark_serialize::CanonicalSerialize;
        
        // nullifier = hash(commitment || secret)
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.commitment.to_bytes());
        
        let mut secret_bytes = [0u8; 32];
        self.nullifier_secret.serialize_compressed(&mut secret_bytes.as_mut_slice()).unwrap();
        hasher.update(&secret_bytes);
        
        *hasher.finalize().as_bytes()
    }
    
    /// Get nullifier secret for proof generation
    pub fn nullifier_secret_bytes(&self) -> [u8; 32] {
        use ark_serialize::CanonicalSerialize;
        let mut bytes = [0u8; 32];
        self.nullifier_secret.serialize_compressed(&mut bytes.as_mut_slice()).unwrap();
        bytes
    }
    
    /// Get blinding factor bytes for proof generation
    pub fn blinding_bytes(&self) -> [u8; 32] {
        use ark_serialize::CanonicalSerialize;
        let mut bytes = [0u8; 32];
        self.blinding.serialize_compressed(&mut bytes.as_mut_slice()).unwrap();
        bytes
    }
}

/// Commitment accumulator (for Merkle tree of commitments)
#[derive(Clone, Debug)]
pub struct CommitmentAccumulator {
    /// All commitments (leaves)
    commitments: Vec<[u8; 32]>,
    /// Merkle tree nodes (cached)
    tree: Vec<[u8; 32]>,
    /// Tree depth
    depth: usize,
}

impl CommitmentAccumulator {
    pub fn new(depth: usize) -> Self {
        let capacity = 1 << depth;
        Self {
            commitments: Vec::with_capacity(capacity),
            tree: vec![[0u8; 32]; 2 * capacity],
            depth,
        }
    }
    
    /// Add a commitment to the accumulator
    pub fn add(&mut self, commitment: &Commitment) -> usize {
        let index = self.commitments.len();
        let bytes = commitment.to_bytes();
        self.commitments.push(bytes);
        
        // Update tree
        self.update_tree(index);
        
        index
    }
    
    /// Get the Merkle root
    pub fn root(&self) -> [u8; 32] {
        if self.tree.is_empty() {
            [0u8; 32]
        } else {
            self.tree[1]
        }
    }
    
    /// Generate Merkle proof for a commitment
    pub fn proof(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.commitments.len() {
            return None;
        }
        
        let mut path = Vec::with_capacity(self.depth);
        let mut path_indices = Vec::with_capacity(self.depth);
        let leaf_offset = 1 << self.depth;
        let mut current = leaf_offset + index;
        
        for _ in 0..self.depth {
            let sibling = if current % 2 == 0 {
                current + 1
            } else {
                current - 1
            };
            
            if sibling < self.tree.len() {
                path.push(self.tree[sibling]);
            } else {
                path.push([0u8; 32]);
            }
            
            path_indices.push(current % 2 == 1);
            current /= 2;
        }
        
        Some(MerkleProof {
            leaf: self.commitments[index],
            path,
            path_indices,
            root: self.root(),
        })
    }
    
    /// Update tree after adding a leaf
    fn update_tree(&mut self, index: usize) {
        let leaf_offset = 1 << self.depth;
        let tree_index = leaf_offset + index;
        
        // Ensure tree is large enough
        while self.tree.len() <= tree_index {
            self.tree.push([0u8; 32]);
        }
        
        // Set leaf
        self.tree[tree_index] = self.commitments[index];
        
        // Update path to root
        let mut current = tree_index;
        while current > 1 {
            let parent = current / 2;
            let left = current - (current % 2);
            let right = left + 1;
            
            // Hash children
            let left_bytes = if left < self.tree.len() { self.tree[left] } else { [0u8; 32] };
            let right_bytes = if right < self.tree.len() { self.tree[right] } else { [0u8; 32] };
            
            let mut hasher = blake3::Hasher::new();
            hasher.update(&left_bytes);
            hasher.update(&right_bytes);
            
            while self.tree.len() <= parent {
                self.tree.push([0u8; 32]);
            }
            self.tree[parent] = *hasher.finalize().as_bytes();
            
            current = parent;
        }
    }
    
    /// Verify a commitment exists in the accumulator
    pub fn verify(&self, proof: &MerkleProof) -> bool {
        proof.verify()
    }
    
    /// Get number of commitments
    pub fn len(&self) -> usize {
        self.commitments.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.commitments.is_empty()
    }
}

/// Merkle proof for commitment inclusion
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: [u8; 32],
    pub path: Vec<[u8; 32]>,
    pub path_indices: Vec<bool>, // true = leaf is on right
    pub root: [u8; 32],
}

impl MerkleProof {
    /// Verify the proof
    pub fn verify(&self) -> bool {
        let mut current = self.leaf;
        
        for (sibling, &is_right) in self.path.iter().zip(self.path_indices.iter()) {
            let mut hasher = blake3::Hasher::new();
            
            if is_right {
                hasher.update(sibling);
                hasher.update(&current);
            } else {
                hasher.update(&current);
                hasher.update(sibling);
            }
            
            current = *hasher.finalize().as_bytes();
        }
        
        current == self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pedersen_commitment() {
        let generators = PedersenGenerators::new();
        let mut rng = ark_std::test_rng();
        
        let value = 100u64;
        let (commitment, blinding) = Commitment::new_random(&generators, value, &mut rng);
        
        assert!(commitment.verify(&generators, value, &blinding));
        assert!(!commitment.verify(&generators, value + 1, &blinding));
    }
    
    #[test]
    fn test_commitment_homomorphism() {
        let generators = PedersenGenerators::new();
        let mut rng = ark_std::test_rng();
        
        let v1 = 100u64;
        let v2 = 50u64;
        
        let (c1, b1) = Commitment::new_random(&generators, v1, &mut rng);
        let (c2, b2) = Commitment::new_random(&generators, v2, &mut rng);
        
        let c_sum = c1.add(&c2);
        let b_sum = b1 + b2;
        
        // c1 + c2 should commit to v1 + v2 with blinding b1 + b2
        assert!(c_sum.verify(&generators, v1 + v2, &b_sum));
    }
    
    #[test]
    fn test_note_nullifier() {
        let generators = PedersenGenerators::new();
        let mut rng = ark_std::test_rng();
        
        let note = Note::new(&generators, 100, [1u8; 32], &mut rng);
        let nullifier = note.nullifier();
        
        // Nullifier should be deterministic
        assert_eq!(nullifier, note.nullifier());
        
        // Different notes should have different nullifiers
        let note2 = Note::new(&generators, 100, [1u8; 32], &mut rng);
        assert_ne!(nullifier, note2.nullifier());
    }
    
    #[test]
    fn test_commitment_accumulator() {
        let generators = PedersenGenerators::new();
        let mut rng = ark_std::test_rng();
        let mut accumulator = CommitmentAccumulator::new(4);
        
        let (c1, _) = Commitment::new_random(&generators, 100, &mut rng);
        let (c2, _) = Commitment::new_random(&generators, 200, &mut rng);
        
        let idx1 = accumulator.add(&c1);
        let idx2 = accumulator.add(&c2);
        
        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        
        // Get and verify proofs
        let proof1 = accumulator.proof(0).unwrap();
        let proof2 = accumulator.proof(1).unwrap();
        
        assert!(proof1.verify());
        assert!(proof2.verify());
        
        // Root should be same in both proofs
        assert_eq!(proof1.root, proof2.root);
    }
}
