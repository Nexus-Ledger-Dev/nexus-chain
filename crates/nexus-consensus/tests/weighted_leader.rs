// Tests for weighted leader selection in BFT consensus

use nexus_consensus::{BftConsensus, ValidatorSet, Validator, BftRoundState};
use nexus_primitives::Address;
use crate::validator::ValidatorKeys;
use std::sync::Arc;
use parking_lot::RwLock;
use k256::ecdsa::SigningKey;

#[test]
fn test_weighted_leader_basic() {
    // Build a validator set with three validators of varying stake
    let mut vs = ValidatorSet::new(0);
    let v1 = Validator::new(Address::from_slice(&[1u8; 20]), vec![], 10);
    let v2 = Validator::new(Address::from_slice(&[2u8; 20]), vec![], 20);
    let v3 = Validator::new(Address::from_slice(&[3u8; 20]), vec![], 30);
    vs.add_validator(v1).unwrap();
    vs.add_validator(v2).unwrap();
    vs.add_validator(v3).unwrap();
    let arc_vs = Arc::new(RwLock::new(vs));

    // Create dummy validator keys (address not relevant for weighted leader)
    let signing_key = SigningKey::random(&mut rand::thread_rng());
    let keys = ValidatorKeys::from_bytes(&signing_key.to_bytes()).unwrap();

    let bft = BftConsensus::new(keys, Arc::clone(&arc_vs));

    // Set a deterministic view and epoch
    *bft.state.write() = BftRoundState { view: 7, epoch: 2, ..Default::default() };

    // Compute the weighted leader
    let leader = bft.weighted_leader().expect("Weighted leader should exist");

    // Verify the leader is one of the known validator addresses
    let expected = vec![
        Address::from_slice(&[1u8; 20]),
        Address::from_slice(&[2u8; 20]),
        Address::from_slice(&[3u8; 20]),
    ];
    assert!(expected.contains(&leader), "Leader must be a validator address");
}
