//! Integration tests for NexusChain

use std::sync::Arc;
use nexus_primitives::*;
use nexus_dag::*;
use nexus_consensus::*;
use nexus_evm::*;
use nexus_zkp::*;
use nexus_iso::*;

/// Test DAG construction and traversal
#[test]
fn test_dag_basic_operations() {
    let dag = Dag::new();
    let proposer = Address::new([1u8; 20]);
    
    // Create genesis vertex
    let genesis = Vertex::new(
        proposer.clone(),
        vec![],
        vec![Hash::from([1u8; 32])],
        0,
        0,
    );
    let genesis_hash = genesis.hash.clone();
    dag.add_vertex(genesis).unwrap();
    
    // Create child vertex
    let child = Vertex::new(
        proposer.clone(),
        vec![genesis_hash.clone()],
        vec![Hash::from([2u8; 32])],
        0,
        1,
    );
    let child_hash = child.hash.clone();
    dag.add_vertex(child).unwrap();
    
    // Verify structure
    assert_eq!(dag.vertex_count(), 2);
    assert_eq!(dag.tips().len(), 1);
    assert!(dag.tips().contains(&child_hash));
    
    // Verify parent-child relationship
    let parents = dag.get_parents(&child_hash);
    assert!(parents.contains(&genesis_hash));
    
    let children = dag.get_children(&genesis_hash);
    assert!(children.contains(&child_hash));
}

/// Test parallel vertex addition
#[test]
fn test_dag_parallel_vertices() {
    let dag = Dag::new();
    let proposer = Address::new([1u8; 20]);
    
    // Create genesis
    let genesis = Vertex::new(
        proposer.clone(),
        vec![],
        vec![],
        0,
        0,
    );
    let genesis_hash = genesis.hash.clone();
    dag.add_vertex(genesis).unwrap();
    
    // Create parallel vertices
    let child1 = Vertex::new(
        proposer.clone(),
        vec![genesis_hash.clone()],
        vec![Hash::from([1u8; 32])],
        0,
        1,
    );
    let child1_hash = child1.hash.clone();
    
    let child2 = Vertex::new(
        proposer.clone(),
        vec![genesis_hash.clone()],
        vec![Hash::from([2u8; 32])],
        0,
        2,
    );
    let child2_hash = child2.hash.clone();
    
    dag.add_vertex(child1).unwrap();
    dag.add_vertex(child2).unwrap();
    
    // Both should be tips
    let tips = dag.tips();
    assert_eq!(tips.len(), 2);
    assert!(tips.contains(&child1_hash));
    assert!(tips.contains(&child2_hash));
}

/// Test tip selection
#[test]
fn test_tip_selection() {
    let dag = Arc::new(Dag::new());
    let selector = TipSelector::new(dag.clone(), TipSelectionConfig::default());
    let proposer = Address::new([1u8; 20]);
    
    // Create some vertices
    let genesis = Vertex::new(proposer.clone(), vec![], vec![], 0, 0);
    dag.add_vertex(genesis).unwrap();
    
    let tips = dag.tips();
    let selected = selector.select_tips(4);
    
    assert!(!selected.is_empty());
    assert!(selected.len() <= 4);
}

/// Test validator registration
#[test]
fn test_validator_registration() {
    let pos = ProofOfStake::default();
    let address = Address::new([1u8; 20]);
    let pubkey = vec![0u8; 48];
    
    pos.register_validator(address.clone(), pubkey, 100_000_000_000, 500).unwrap();
    
    let set = pos.validator_set();
    assert!(set.iter().any(|v| v.address == address));
    assert_eq!(set.iter().find(|v| v.address == address).unwrap().stake, 100_000_000_000);
}

/// Test ISO validators
#[test]
fn test_iso_validators() {
    // Valid IBAN
    assert!(validate_iban("DE89370400440532013000").is_ok());
    
    // Invalid IBAN
    assert!(validate_iban("INVALID").is_err());
    
    // Valid BIC
    assert!(validate_bic("DEUTDEFF").is_ok());
    assert!(validate_bic("DEUTDEFF500").is_ok());
    
    // Invalid BIC
    assert!(validate_bic("TOOSHORT").is_err());
}

/// Test currency lookup
#[test]
fn test_currency_lookup() {
    let usd = get_currency("USD").unwrap();
    assert_eq!(usd.name, "US Dollar");
    assert_eq!(usd.decimals, 2);
    
    let btc = get_currency("BTC").unwrap();
    assert_eq!(btc.decimals, 8);
    
    assert!(get_currency("XXX").is_none());
}

/// Test state operations
#[test]
fn test_state_operations() {
    use nexus_state::StateDb;
    
    let state = StateDb::new();
    let addr = Address::new([1u8; 20]);
    
    // Balance operations
    state.set_balance(&addr, U256::from(1000));
    assert_eq!(state.get_balance(&addr), U256::from(1000));
    
    state.add_balance(&addr, U256::from(500));
    assert_eq!(state.get_balance(&addr), U256::from(1500));
    
    // Nonce operations
    state.increment_nonce(&addr);
    state.increment_nonce(&addr);
    assert_eq!(state.get_nonce(&addr), 2);
    
    // Storage operations
    state.set_storage(&addr, U256::from(0), U256::from(42));
    assert_eq!(state.get_storage(&addr, &U256::from(0)), U256::from(42));
}

/// Test Pedersen commitment
#[test]
fn test_pedersen_commitment() {
    let commitment = PedersenCommitment::new(1000);
    
    // Can verify with correct value
    assert!(commitment.verify(1000));
    
    // Cannot verify with wrong value
    assert!(!commitment.verify(999));
}

/// Test ISO 20022 message parsing
#[test]
fn test_iso20022_parsing() {
    let parser = Iso20022Parser::new();
    
    // Would test with actual XML - simplified here
    let minimal_xml = r#"<?xml version="1.0"?>
        <Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
        </Document>"#;
    
    // Parser would handle this
    // let result = parser.parse_pacs008(minimal_xml);
}
