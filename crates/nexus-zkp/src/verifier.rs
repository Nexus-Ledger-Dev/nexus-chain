//! ZKP Verifier implementation

use crate::{prover::SerializedProof, ZkpError, ZkpResult};
use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, PreparedVerifyingKey, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use nexus_primitives::Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, warn};

/// Verifying keys for different circuit types
#[derive(Clone)]
pub struct VerifyingKeys {
    pub private_tx: Option<Arc<PreparedVerifyingKey<Bn254>>>,
    pub balance_proof: Option<Arc<PreparedVerifyingKey<Bn254>>>,
    pub compliance: Option<Arc<PreparedVerifyingKey<Bn254>>>,
}

impl VerifyingKeys {
    pub fn new() -> Self {
        Self {
            private_tx: None,
            balance_proof: None,
            compliance: None,
        }
    }
    
    /// Load from serialized verifying keys
    pub fn from_bytes(
        private_tx: Option<&[u8]>,
        balance_proof: Option<&[u8]>,
        compliance: Option<&[u8]>,
    ) -> ZkpResult<Self> {
        let load_vk = |bytes: &[u8]| -> ZkpResult<PreparedVerifyingKey<Bn254>> {
            let vk = VerifyingKey::deserialize_compressed(bytes)
                .map_err(|e| ZkpError::SerializationError(e.to_string()))?;
            Ok(Groth16::<Bn254>::process_vk(&vk)
                .map_err(|e| ZkpError::VerificationFailed(e.to_string()))?)
        };
        
        Ok(Self {
            private_tx: private_tx.map(|b| load_vk(b).map(Arc::new)).transpose()?,
            balance_proof: balance_proof.map(|b| load_vk(b).map(Arc::new)).transpose()?,
            compliance: compliance.map(|b| load_vk(b).map(Arc::new)).transpose()?,
        })
    }
}

impl Default for VerifyingKeys {
    fn default() -> Self {
        Self::new()
    }
}

/// Nullifier set for double-spend prevention
pub struct NullifierSet {
    /// In-memory nullifiers (would be persisted in production)
    nullifiers: RwLock<HashSet<[u8; 32]>>,
}

impl NullifierSet {
    pub fn new() -> Self {
        Self {
            nullifiers: RwLock::new(HashSet::new()),
        }
    }
    
    /// Check if nullifier exists
    pub fn contains(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifiers.read().contains(nullifier)
    }
    
    /// Add nullifier (returns false if already exists)
    pub fn insert(&self, nullifier: [u8; 32]) -> bool {
        self.nullifiers.write().insert(nullifier)
    }
    
    /// Remove nullifier (for rollbacks)
    pub fn remove(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifiers.write().remove(nullifier)
    }
    
    /// Get count
    pub fn len(&self) -> usize {
        self.nullifiers.read().len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.nullifiers.read().is_empty()
    }
}

impl Default for NullifierSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Verification result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationResult {
    pub valid: bool,
    pub proof_type: ProofType,
    pub nullifier: Option<[u8; 32]>,
    pub verification_time_us: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofType {
    PrivateTransaction,
    BalanceProof,
    Compliance,
    MerkleInclusion,
}

/// The main verifier
pub struct Verifier {
    keys: VerifyingKeys,
    nullifier_set: NullifierSet,
}

impl Verifier {
    pub fn new(keys: VerifyingKeys) -> Self {
        Self {
            keys,
            nullifier_set: NullifierSet::new(),
        }
    }
    
    /// Verify a private transaction proof
    pub fn verify_private_transaction(&self, proof: &SerializedProof) -> ZkpResult<VerificationResult> {
        let start = std::time::Instant::now();
        
        let pvk = self.keys.private_tx.as_ref()
            .ok_or_else(|| ZkpError::VerificationFailed("Verifying key not loaded".into()))?;
        
        let (groth_proof, public_inputs) = proof.to_groth16()?;
        
        // Extract nullifier from public inputs (3rd input)
        let nullifier = if public_inputs.len() >= 3 {
            let mut bytes = [0u8; 32];
            public_inputs[2].serialize_compressed(&mut bytes.as_mut_slice())
                .map_err(|e| ZkpError::SerializationError(e.to_string()))?;
            Some(bytes)
        } else {
            None
        };
        
        // Check nullifier hasn't been spent
        if let Some(ref nf) = nullifier {
            if self.nullifier_set.contains(nf) {
                return Err(ZkpError::NullifierSpent);
            }
        }
        
        // Verify the proof
        let valid = Groth16::<Bn254>::verify_with_processed_vk(pvk, &public_inputs, &groth_proof)
            .map_err(|e| ZkpError::VerificationFailed(e.to_string()))?;
        
        let verification_time_us = start.elapsed().as_micros() as u64;
        
        debug!("Private transaction proof verified: {} in {}μs", valid, verification_time_us);
        
        Ok(VerificationResult {
            valid,
            proof_type: ProofType::PrivateTransaction,
            nullifier,
            verification_time_us,
        })
    }
    
    /// Verify a balance proof
    pub fn verify_balance_proof(&self, proof: &SerializedProof) -> ZkpResult<VerificationResult> {
        let start = std::time::Instant::now();
        
        let pvk = self.keys.balance_proof.as_ref()
            .ok_or_else(|| ZkpError::VerificationFailed("Verifying key not loaded".into()))?;
        
        let (groth_proof, public_inputs) = proof.to_groth16()?;
        
        let valid = Groth16::<Bn254>::verify_with_processed_vk(pvk, &public_inputs, &groth_proof)
            .map_err(|e| ZkpError::VerificationFailed(e.to_string()))?;
        
        let verification_time_us = start.elapsed().as_micros() as u64;
        
        Ok(VerificationResult {
            valid,
            proof_type: ProofType::BalanceProof,
            nullifier: None,
            verification_time_us,
        })
    }
    
    /// Verify a compliance proof
    pub fn verify_compliance_proof(&self, proof: &SerializedProof) -> ZkpResult<VerificationResult> {
        let start = std::time::Instant::now();
        
        let pvk = self.keys.compliance.as_ref()
            .ok_or_else(|| ZkpError::VerificationFailed("Verifying key not loaded".into()))?;
        
        let (groth_proof, public_inputs) = proof.to_groth16()?;
        
        let valid = Groth16::<Bn254>::verify_with_processed_vk(pvk, &public_inputs, &groth_proof)
            .map_err(|e| ZkpError::VerificationFailed(e.to_string()))?;
        
        let verification_time_us = start.elapsed().as_micros() as u64;
        
        Ok(VerificationResult {
            valid,
            proof_type: ProofType::Compliance,
            nullifier: None,
            verification_time_us,
        })
    }
    
    /// Mark nullifier as spent (after successful transaction)
    pub fn mark_nullifier_spent(&self, nullifier: [u8; 32]) -> bool {
        self.nullifier_set.insert(nullifier)
    }
    
    /// Rollback nullifier (for failed transactions)
    pub fn rollback_nullifier(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifier_set.remove(nullifier)
    }
    
    /// Get nullifier count
    pub fn nullifier_count(&self) -> usize {
        self.nullifier_set.len()
    }
}

/// Batch verifier for multiple proofs
pub struct BatchVerifier {
    verifier: Arc<Verifier>,
}

impl BatchVerifier {
    pub fn new(verifier: Arc<Verifier>) -> Self {
        Self { verifier }
    }
    
    /// Verify multiple proofs in batch
    pub fn verify_batch(&self, proofs: &[(SerializedProof, ProofType)]) -> Vec<ZkpResult<VerificationResult>> {
        // In a real implementation, batch verification would use
        // techniques like random linear combinations for efficiency
        proofs.iter()
            .map(|(proof, proof_type)| {
                match proof_type {
                    ProofType::PrivateTransaction => self.verifier.verify_private_transaction(proof),
                    ProofType::BalanceProof => self.verifier.verify_balance_proof(proof),
                    ProofType::Compliance => self.verifier.verify_compliance_proof(proof),
                    ProofType::MerkleInclusion => {
                        // Would implement merkle verification
                        Err(ZkpError::VerificationFailed("Merkle verification not implemented".into()))
                    }
                }
            })
            .collect()
    }
}

/// EVM precompile interface for ZKP verification
pub mod precompile {
    use super::*;
    
    /// Gas costs for ZKP operations
    pub const GROTH16_VERIFY_GAS: u64 = 100_000;
    pub const BALANCE_PROOF_GAS: u64 = 80_000;
    pub const COMPLIANCE_PROOF_GAS: u64 = 120_000;
    
    /// Precompile input format for Groth16 verification
    #[derive(Clone, Debug)]
    pub struct Groth16VerifyInput {
        pub proof: Vec<u8>,
        pub public_inputs: Vec<[u8; 32]>,
        pub vk_hash: [u8; 32], // Hash of verifying key for lookup
    }
    
    impl Groth16VerifyInput {
        /// Parse from EVM calldata
        pub fn from_calldata(data: &[u8]) -> ZkpResult<Self> {
            if data.len() < 64 {
                return Err(ZkpError::InvalidInput("Calldata too short".into()));
            }
            
            // Format: vk_hash (32) + proof_len (32) + proof + inputs
            let mut vk_hash = [0u8; 32];
            vk_hash.copy_from_slice(&data[0..32]);
            
            let proof_len = u64::from_be_bytes(data[32..40].try_into().unwrap()) as usize;
            
            if data.len() < 40 + proof_len {
                return Err(ZkpError::InvalidInput("Invalid proof length".into()));
            }
            
            let proof = data[40..40 + proof_len].to_vec();
            
            // Parse remaining as public inputs
            let inputs_data = &data[40 + proof_len..];
            let num_inputs = inputs_data.len() / 32;
            
            let public_inputs: Vec<[u8; 32]> = (0..num_inputs)
                .map(|i| {
                    let mut input = [0u8; 32];
                    input.copy_from_slice(&inputs_data[i * 32..(i + 1) * 32]);
                    input
                })
                .collect();
            
            Ok(Self {
                proof,
                public_inputs,
                vk_hash,
            })
        }
    }
    
    /// Execute the ZKP verification precompile
    pub fn execute_verify(
        verifier: &Verifier,
        input: &Groth16VerifyInput,
    ) -> ZkpResult<Vec<u8>> {
        let proof = SerializedProof {
            proof_bytes: input.proof.clone(),
            public_inputs: input.public_inputs.clone(),
        };
        
        // Determine proof type from vk_hash (simplified - would use lookup table)
        let result = verifier.verify_private_transaction(&proof)?;
        
        // Return 32-byte result: 1 if valid, 0 if invalid
        let mut output = [0u8; 32];
        if result.valid {
            output[31] = 1;
        }
        
        Ok(output.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_nullifier_set() {
        let set = NullifierSet::new();
        let nullifier = [1u8; 32];
        
        assert!(!set.contains(&nullifier));
        assert!(set.insert(nullifier));
        assert!(set.contains(&nullifier));
        assert!(!set.insert(nullifier)); // Already exists
        assert!(set.remove(&nullifier));
        assert!(!set.contains(&nullifier));
    }
    
    #[test]
    fn test_precompile_input_parsing() {
        let mut data = Vec::new();
        
        // vk_hash
        data.extend_from_slice(&[0u8; 32]);
        
        // proof_len (8 bytes, big endian)
        data.extend_from_slice(&8u64.to_be_bytes());
        
        // proof (8 bytes)
        data.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        
        // public input (32 bytes)
        data.extend_from_slice(&[0xAB; 32]);
        
        let input = precompile::Groth16VerifyInput::from_calldata(&data).unwrap();
        
        assert_eq!(input.proof.len(), 8);
        assert_eq!(input.public_inputs.len(), 1);
        assert_eq!(input.public_inputs[0], [0xAB; 32]);
    }
}
