//! Poseidon hash function for ZK-friendly hashing
//!
//! Poseidon is designed for efficient in-circuit computation,
//! using fewer constraints than SHA256 or Keccak.

use ark_ff::PrimeField;
use ark_bn254::Fr;

/// Poseidon hash parameters for BN254
pub struct PoseidonParams {
    /// Round constants
    pub round_constants: Vec<Fr>,
    /// MDS matrix
    pub mds_matrix: Vec<Vec<Fr>>,
    /// Number of full rounds
    pub full_rounds: usize,
    /// Number of partial rounds
    pub partial_rounds: usize,
    /// State width (t)
    pub width: usize,
}

impl PoseidonParams {
    /// Create parameters for width 3 (2 inputs + 1 capacity)
    pub fn new_width3() -> Self {
        // These would be generated via the Poseidon specification
        // Using placeholder values for demonstration
        let width = 3;
        let full_rounds = 8;
        let partial_rounds = 57;
        
        let total_rounds = full_rounds + partial_rounds;
        let round_constants: Vec<Fr> = (0..total_rounds * width)
            .map(|i| Fr::from(i as u64 + 1))
            .collect();
        
        // Simple MDS matrix (would be properly generated in production)
        let mds_matrix = vec![
            vec![Fr::from(2u64), Fr::from(1u64), Fr::from(1u64)],
            vec![Fr::from(1u64), Fr::from(2u64), Fr::from(1u64)],
            vec![Fr::from(1u64), Fr::from(1u64), Fr::from(2u64)],
        ];
        
        Self {
            round_constants,
            mds_matrix,
            full_rounds,
            partial_rounds,
            width,
        }
    }
}

/// Poseidon hash function
pub struct Poseidon {
    params: PoseidonParams,
}

impl Poseidon {
    pub fn new(params: PoseidonParams) -> Self {
        Self { params }
    }
    
    /// Hash two field elements
    pub fn hash2(&self, a: &Fr, b: &Fr) -> Fr {
        let mut state = vec![Fr::from(0u64), *a, *b];
        self.permute(&mut state);
        state[0]
    }
    
    /// Hash arbitrary number of field elements
    pub fn hash(&self, inputs: &[Fr]) -> Fr {
        if inputs.is_empty() {
            return Fr::from(0u64);
        }
        
        // Sponge construction
        let rate = self.params.width - 1; // capacity = 1
        let mut state = vec![Fr::from(0u64); self.params.width];
        
        // Absorb
        for chunk in inputs.chunks(rate) {
            for (i, &input) in chunk.iter().enumerate() {
                state[i + 1] = state[i + 1] + input;
            }
            self.permute(&mut state);
        }
        
        // Squeeze (single element)
        state[0]
    }
    
    /// Poseidon permutation
    fn permute(&self, state: &mut [Fr]) {
        let half_full = self.params.full_rounds / 2;
        let mut round_constant_idx = 0;
        
        // First half of full rounds
        for _ in 0..half_full {
            self.full_round(state, &mut round_constant_idx);
        }
        
        // Partial rounds
        for _ in 0..self.params.partial_rounds {
            self.partial_round(state, &mut round_constant_idx);
        }
        
        // Second half of full rounds
        for _ in 0..half_full {
            self.full_round(state, &mut round_constant_idx);
        }
    }
    
    /// Full round: S-box on all elements
    fn full_round(&self, state: &mut [Fr], rc_idx: &mut usize) {
        // Add round constants
        for i in 0..self.params.width {
            state[i] = state[i] + self.params.round_constants[*rc_idx];
            *rc_idx += 1;
        }
        
        // S-box (x^5) on all elements
        for i in 0..self.params.width {
            state[i] = self.sbox(&state[i]);
        }
        
        // Linear layer (MDS)
        self.mds_mix(state);
    }
    
    /// Partial round: S-box only on first element
    fn partial_round(&self, state: &mut [Fr], rc_idx: &mut usize) {
        // Add round constants
        for i in 0..self.params.width {
            state[i] = state[i] + self.params.round_constants[*rc_idx];
            *rc_idx += 1;
        }
        
        // S-box only on first element
        state[0] = self.sbox(&state[0]);
        
        // Linear layer
        self.mds_mix(state);
    }
    
    /// S-box: x^5
    fn sbox(&self, x: &Fr) -> Fr {
        let x2 = *x * x;
        let x4 = x2 * x2;
        x4 * x
    }
    
    /// MDS matrix multiplication
    fn mds_mix(&self, state: &mut [Fr]) {
        let mut new_state = vec![Fr::from(0u64); self.params.width];
        
        for i in 0..self.params.width {
            for j in 0..self.params.width {
                new_state[i] = new_state[i] + (self.params.mds_matrix[i][j] * state[j]);
            }
        }
        
        state.copy_from_slice(&new_state);
    }
}

impl Default for Poseidon {
    fn default() -> Self {
        Self::new(PoseidonParams::new_width3())
    }
}

/// Poseidon-based Merkle tree
pub struct PoseidonMerkle {
    poseidon: Poseidon,
    depth: usize,
}

impl PoseidonMerkle {
    pub fn new(depth: usize) -> Self {
        Self {
            poseidon: Poseidon::default(),
            depth,
        }
    }
    
    /// Hash two children to get parent
    pub fn hash_pair(&self, left: &Fr, right: &Fr) -> Fr {
        self.poseidon.hash2(left, right)
    }
    
    /// Compute Merkle root from leaves
    pub fn compute_root(&self, leaves: &[Fr]) -> Fr {
        if leaves.is_empty() {
            return Fr::from(0u64);
        }
        
        let mut current_level = leaves.to_vec();
        
        // Pad to power of 2
        let target_size = 1 << self.depth;
        while current_level.len() < target_size {
            current_level.push(Fr::from(0u64));
        }
        
        // Build tree bottom-up
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            
            for chunk in current_level.chunks(2) {
                let left = chunk[0];
                let right = if chunk.len() > 1 { chunk[1] } else { Fr::from(0u64) };
                next_level.push(self.hash_pair(&left, &right));
            }
            
            current_level = next_level;
        }
        
        current_level[0]
    }
    
    /// Verify a Merkle proof
    pub fn verify_proof(
        &self,
        leaf: &Fr,
        path: &[Fr],
        path_indices: &[bool],
        root: &Fr,
    ) -> bool {
        if path.len() != self.depth || path_indices.len() != self.depth {
            return false;
        }
        
        let mut current = *leaf;
        
        for (sibling, &is_right) in path.iter().zip(path_indices.iter()) {
            current = if is_right {
                self.hash_pair(sibling, &current)
            } else {
                self.hash_pair(&current, sibling)
            };
        }
        
        current == *root
    }
}

/// Convert bytes to field element
pub fn bytes_to_field(bytes: &[u8]) -> Fr {
    Fr::from_le_bytes_mod_order(bytes)
}

/// Convert field element to bytes
pub fn field_to_bytes(field: &Fr) -> [u8; 32] {
    use ark_serialize::CanonicalSerialize;
    let mut bytes = [0u8; 32];
    field.serialize_compressed(&mut bytes.as_mut_slice()).unwrap();
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_poseidon_hash() {
        let poseidon = Poseidon::default();
        
        let a = Fr::from(1u64);
        let b = Fr::from(2u64);
        
        let hash1 = poseidon.hash2(&a, &b);
        let hash2 = poseidon.hash2(&a, &b);
        
        // Same inputs should give same output
        assert_eq!(hash1, hash2);
        
        // Different inputs should give different output
        let hash3 = poseidon.hash2(&b, &a);
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_poseidon_merkle() {
        let merkle = PoseidonMerkle::new(3);
        
        let leaves: Vec<Fr> = (0..4)
            .map(|i| Fr::from(i as u64))
            .collect();
        
        let root = merkle.compute_root(&leaves);
        
        // Root should be deterministic
        let root2 = merkle.compute_root(&leaves);
        assert_eq!(root, root2);
    }
    
    #[test]
    fn test_bytes_conversion() {
        let original = Fr::from(12345u64);
        let bytes = field_to_bytes(&original);
        let recovered = bytes_to_field(&bytes);
        
        assert_eq!(original, recovered);
    }
}
