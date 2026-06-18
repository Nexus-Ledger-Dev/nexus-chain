use crate::prover::SerializedProof;
use crate::verifier::{Verifier as InnerVerifier, VerifyingKeys};
use std::sync::Arc;

pub trait Verifier: Send + Sync {
    fn verify(&self, proof: &[u8], input: &[u8]) -> bool;
}

/// Bridges the raw-bytes `Verifier` trait to the typed Groth16 verifier.
///
/// `proof`  — compressed ark-serialize Groth16 proof bytes.
/// `input`  — concatenated 32-byte big-endian field elements (public inputs).
///
/// Returns `false` whenever verifying keys have not been loaded, providing a
/// fail-safe default rather than blindly accepting any non-empty byte slice.
pub struct DefaultVerifier {
    inner: Option<Arc<InnerVerifier>>,
}

impl Default for DefaultVerifier {
    fn default() -> Self {
        Self { inner: None }
    }
}

impl DefaultVerifier {
    /// Create a verifier backed by real Groth16 verifying keys.
    pub fn with_keys(keys: VerifyingKeys) -> Self {
        Self {
            inner: Some(Arc::new(InnerVerifier::new(keys))),
        }
    }
}

impl Verifier for DefaultVerifier {
    fn verify(&self, proof: &[u8], input: &[u8]) -> bool {
        let Some(verifier) = &self.inner else {
            // No verifying keys loaded — reject rather than accept blindly.
            return false;
        };

        // Decode input as concatenated 32-byte field elements.
        if input.len() % 32 != 0 {
            return false;
        }
        let public_inputs: Vec<[u8; 32]> = input
            .chunks_exact(32)
            .map(|c| c.try_into().expect("chunk is exactly 32 bytes"))
            .collect();

        let serialized = SerializedProof {
            proof_bytes: proof.to_vec(),
            public_inputs,
        };

        verifier
            .verify_private_transaction(&serialized)
            .map(|r| r.valid)
            .unwrap_or(false)
    }
}
