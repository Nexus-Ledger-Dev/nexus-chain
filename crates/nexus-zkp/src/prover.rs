//! Groth16 prover implementation

use crate::{
    circuits::{PrivateTransactionCircuit, BalanceProofCircuit, ComplianceCircuit},
    ZkpError, ZkpResult,
};
use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, ProvingKey};
use ark_snark::SNARK;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

/// Serialized proof data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedProof {
    pub proof_bytes: Vec<u8>,
    pub public_inputs: Vec<[u8; 32]>,
}

impl SerializedProof {
    pub fn from_groth16(proof: &Proof<Bn254>, public_inputs: &[Fr]) -> ZkpResult<Self> {
        let mut proof_bytes = Vec::new();
        proof.serialize_compressed(&mut proof_bytes)
            .map_err(|e| ZkpError::SerializationError(e.to_string()))?;
        
        let public_inputs: Vec<[u8; 32]> = public_inputs.iter()
            .map(|input| {
                let mut bytes = [0u8; 32];
                input.serialize_compressed(&mut bytes.as_mut_slice())
                    .expect("Fr fits in 32 bytes");
                bytes
            })
            .collect();
        
        Ok(Self {
            proof_bytes,
            public_inputs,
        })
    }
    
    pub fn to_groth16(&self) -> ZkpResult<(Proof<Bn254>, Vec<Fr>)> {
        let proof = Proof::deserialize_compressed(&self.proof_bytes[..])
            .map_err(|e| ZkpError::SerializationError(e.to_string()))?;
        
        let public_inputs: Result<Vec<Fr>, _> = self.public_inputs.iter()
            .map(|bytes| Fr::deserialize_compressed(&bytes[..]))
            .collect();
        
        let public_inputs = public_inputs
            .map_err(|e| ZkpError::SerializationError(e.to_string()))?;
        
        Ok((proof, public_inputs))
    }
}

/// Proving keys for different circuit types
pub struct ProvingKeys {
    pub private_tx: Option<Arc<ProvingKey<Bn254>>>,
    pub balance_proof: Option<Arc<ProvingKey<Bn254>>>,
    pub compliance: Option<Arc<ProvingKey<Bn254>>>,
}

impl ProvingKeys {
    pub fn new() -> Self {
        Self {
            private_tx: None,
            balance_proof: None,
            compliance: None,
        }
    }
    
    /// Generate proving keys (trusted setup)
    pub fn setup(&mut self) -> ZkpResult<()> {
        let mut rng = thread_rng();
        
        info!("Generating proving key for private transaction circuit...");
        let circuit = PrivateTransactionCircuit::<Fr>::empty();
        let (pk, _vk) = Groth16::<Bn254>::circuit_specific_setup(circuit, &mut rng)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;
        self.private_tx = Some(Arc::new(pk));
        
        info!("Generating proving key for balance proof circuit...");
        let circuit = BalanceProofCircuit::<Fr>::empty();
        let (pk, _vk) = Groth16::<Bn254>::circuit_specific_setup(circuit, &mut rng)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;
        self.balance_proof = Some(Arc::new(pk));
        
        info!("Generating proving key for compliance circuit...");
        let circuit = ComplianceCircuit::<Fr>::empty();
        let (pk, _vk) = Groth16::<Bn254>::circuit_specific_setup(circuit, &mut rng)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;
        self.compliance = Some(Arc::new(pk));
        
        info!("All proving keys generated");
        Ok(())
    }
    
    /// Serialize proving keys for storage
    pub fn serialize(&self) -> ZkpResult<Vec<u8>> {
        let mut buffer = Vec::new();
        
        if let Some(pk) = &self.private_tx {
            pk.serialize_compressed(&mut buffer)
                .map_err(|e| ZkpError::SerializationError(e.to_string()))?;
        }
        
        Ok(buffer)
    }
}

impl Default for ProvingKeys {
    fn default() -> Self {
        Self::new()
    }
}

/// The main prover
pub struct Prover {
    keys: ProvingKeys,
}

impl Prover {
    pub fn new(keys: ProvingKeys) -> Self {
        Self { keys }
    }
    
    /// Prove a private transaction
    pub fn prove_private_transaction(
        &self,
        input_value: u64,
        input_blinding: [u8; 32],
        input_commitment: [u8; 32],
        output_value: u64,
        output_blinding: [u8; 32],
        output_commitment: [u8; 32],
        nullifier: [u8; 32],
        nullifier_secret: [u8; 32],
        fee: u64,
    ) -> ZkpResult<SerializedProof> {
        let pk = self.keys.private_tx.as_ref()
            .ok_or_else(|| ZkpError::ProofGenerationFailed("Proving key not loaded".into()))?;
        
        let circuit = PrivateTransactionCircuit::new(
            Fr::from(input_value),
            bytes_to_fr(&input_blinding),
            bytes_to_fr(&input_commitment),
            Fr::from(output_value),
            bytes_to_fr(&output_blinding),
            bytes_to_fr(&output_commitment),
            bytes_to_fr(&nullifier),
            bytes_to_fr(&nullifier_secret),
            Fr::from(fee),
        );
        
        let mut rng = thread_rng();
        let proof = Groth16::<Bn254>::prove(pk.as_ref(), circuit, &mut rng)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;
        
        let public_inputs = vec![
            bytes_to_fr(&input_commitment),
            bytes_to_fr(&output_commitment),
            bytes_to_fr(&nullifier),
            Fr::from(fee),
        ];
        
        SerializedProof::from_groth16(&proof, &public_inputs)
    }
    
    /// Prove balance sufficiency
    pub fn prove_balance(
        &self,
        balance: u64,
        balance_commitment: [u8; 32],
        minimum_required: u64,
        blinding: [u8; 32],
    ) -> ZkpResult<SerializedProof> {
        let pk = self.keys.balance_proof.as_ref()
            .ok_or_else(|| ZkpError::ProofGenerationFailed("Proving key not loaded".into()))?;
        
        let circuit = BalanceProofCircuit::new(
            Fr::from(balance),
            bytes_to_fr(&balance_commitment),
            Fr::from(minimum_required),
            bytes_to_fr(&blinding),
        );
        
        let mut rng = thread_rng();
        let proof = Groth16::<Bn254>::prove(pk.as_ref(), circuit, &mut rng)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;
        
        let public_inputs = vec![
            bytes_to_fr(&balance_commitment),
            Fr::from(minimum_required),
        ];
        
        SerializedProof::from_groth16(&proof, &public_inputs)
    }
}

/// Convert 32 bytes to field element
fn bytes_to_fr(bytes: &[u8; 32]) -> Fr {
    // Take first 31 bytes to ensure we're within the field
    let mut truncated = [0u8; 31];
    truncated.copy_from_slice(&bytes[..31]);
    Fr::from_le_bytes_mod_order(&truncated)
}

/// Batch prover for multiple proofs
pub struct BatchProver {
    prover: Prover,
    pending: Vec<PendingProof>,
}

#[derive(Clone)]
enum PendingProof {
    PrivateTransaction {
        input_value: u64,
        input_blinding: [u8; 32],
        input_commitment: [u8; 32],
        output_value: u64,
        output_blinding: [u8; 32],
        output_commitment: [u8; 32],
        nullifier: [u8; 32],
        nullifier_secret: [u8; 32],
        fee: u64,
    },
    Balance {
        balance: u64,
        commitment: [u8; 32],
        minimum: u64,
        blinding: [u8; 32],
    },
}

impl BatchProver {
    pub fn new(prover: Prover) -> Self {
        Self {
            prover,
            pending: Vec::new(),
        }
    }
    
    /// Add a private transaction proof to the batch
    pub fn add_private_transaction(
        &mut self,
        input_value: u64,
        input_blinding: [u8; 32],
        input_commitment: [u8; 32],
        output_value: u64,
        output_blinding: [u8; 32],
        output_commitment: [u8; 32],
        nullifier: [u8; 32],
        nullifier_secret: [u8; 32],
        fee: u64,
    ) {
        self.pending.push(PendingProof::PrivateTransaction {
            input_value,
            input_blinding,
            input_commitment,
            output_value,
            output_blinding,
            output_commitment,
            nullifier,
            nullifier_secret,
            fee,
        });
    }
    
    /// Add a balance proof to the batch
    pub fn add_balance_proof(
        &mut self,
        balance: u64,
        commitment: [u8; 32],
        minimum: u64,
        blinding: [u8; 32],
    ) {
        self.pending.push(PendingProof::Balance {
            balance,
            commitment,
            minimum,
            blinding,
        });
    }
    
    /// Generate all pending proofs
    pub fn prove_all(&mut self) -> ZkpResult<Vec<SerializedProof>> {
        let proofs: Vec<ZkpResult<SerializedProof>> = self.pending.drain(..)
            .map(|pending| match pending {
                PendingProof::PrivateTransaction {
                    input_value,
                    input_blinding,
                    input_commitment,
                    output_value,
                    output_blinding,
                    output_commitment,
                    nullifier,
                    nullifier_secret,
                    fee,
                } => self.prover.prove_private_transaction(
                    input_value,
                    input_blinding,
                    input_commitment,
                    output_value,
                    output_blinding,
                    output_commitment,
                    nullifier,
                    nullifier_secret,
                    fee,
                ),
                PendingProof::Balance {
                    balance,
                    commitment,
                    minimum,
                    blinding,
                } => self.prover.prove_balance(balance, commitment, minimum, blinding),
            })
            .collect();
        
        proofs.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_serialized_proof_roundtrip() {
        // This would require actual proof generation
        // Simplified test for serialization logic
        let proof = SerializedProof {
            proof_bytes: vec![1, 2, 3, 4],
            public_inputs: vec![[0u8; 32]],
        };
        
        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: SerializedProof = serde_json::from_str(&json).unwrap();
        
        assert_eq!(proof.proof_bytes, deserialized.proof_bytes);
    }
}
