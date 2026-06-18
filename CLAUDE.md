# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Check compilation (fast, no linking)
cargo check

# Check a single crate
cargo check -p nexus-consensus

# Build all (debug)
cargo build

# Build with optional features
cargo build -p nexus-node --features "zkp,iso-compliance,p2p-network,rocksdb"

# Build release
cargo build --release

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p nexus-consensus

# Run a single test by name
cargo test -p nexus-consensus test_weighted_leader_basic

# Run workspace integration tests
cargo test --test integration_tests

# Run ZKP benchmarks
cargo bench -p nexus-zkp

# Run the node binary
cargo run -p nexus-node -- run --rpc-addr 127.0.0.1:8545
```

## Node CLI Commands

```bash
# Initialize data directory
./target/release/nexus init --data-dir ~/.nexus --chain-id 1337

# Run as non-validator
./target/release/nexus run --rpc-addr 127.0.0.1:8545

# Generate validator key
./target/release/nexus generate-key --output ~/.nexus/validator.key

# Run as validator
./target/release/nexus run --validator --validator-key ~/.nexus/validator.key

# Query node status
./target/release/nexus status --rpc http://127.0.0.1:8545

# Run DAG benchmarks
./target/release/nexus benchmark --bench-type dag --iterations 10000
```

## Architecture

NexusChain is a modular Layer 1 blockchain targeting financial services compliance. It combines DAG-based transaction ordering with a BFT finality gadget, full EVM compatibility, ZKP integration, and ISO 20022/8583 banking standard support.

### Crate Dependency Flow

```
nexus-primitives
    └── nexus-dag, nexus-zkp, nexus-state
            └── nexus-consensus (also depends on nexus-dag, nexus-zkp)
                    └── nexus-evm (optionally nexus-zkp, nexus-iso)
                            └── nexus-iso
                                    └── nexus-network, nexus-rpc
                                            └── nexus-node (binary)
```

### Crate Responsibilities

| Crate | Role |
|---|---|
| `nexus-primitives` | Foundation types: `Hash`, `Address`, `Transaction`, crypto primitives, precompile address constants |
| `nexus-dag` | DAG data structure: `Vertex`, `Dag`, tip selection, weight accumulation, BFT finality gadget |
| `nexus-consensus` | PoS validator set, BFT three-phase commit, hybrid consensus (weighted leader election), ISO audit logger |
| `nexus-evm` | `revm`-based EVM executor, custom precompiles (ZKP 0x20–0x2F, ISO 0x30–0x3F), gas metering, state DB |
| `nexus-zkp` | Arkworks Groth16/PLONK circuits, Poseidon hash, commitment scheme, proof generation/verification |
| `nexus-iso` | ISO 20022 message routing (pain.*/pacs.*/camt.*), ISO 8583 card transaction parsing, validators |
| `nexus-state` | In-memory account/contract state trie backed by RocksDB (optional) |
| `nexus-network` | libp2p gossip, Kademlia DHT peer discovery, sync protocol |
| `nexus-rpc` | Axum-based JSON-RPC server; exposes Ethereum-compatible (`eth_*`) and NexusChain-specific (`nexus_*`) methods; includes in-memory mempool |
| `nexus-node` | Binary: assembles all components, handles CLI (`init`, `run`, `generate-key`, `status`, `benchmark`), genesis import/export |

### Key Design Patterns

**Consensus (hybrid BFT + DAG)**
- `nexus-consensus` has a `hybrid_consensus` Cargo feature that enables `HybridConsensus` — weighted PoS leader election layered over `BftConsensus`. Compile with `--features hybrid_consensus` to activate.
- BFT uses a three-phase protocol (prepare → commit → finalize). The `ValidatorSet` tracks stake; `ValidatorKeys` wraps a k256 ECDSA signing key.
- DAG tips (`Vertex` with no children) are the frontier; each new vertex references up to `MAX_PARENTS = 8` tips. Weight accumulates from descendants toward finality.

**EVM Precompiles**
- ZKP precompiles at addresses `0x20`–`0x2F`: Groth16 verify, PLONK verify, Poseidon hash, commitment.
- ISO precompiles at `0x30`–`0x3F`: ISO 20022 validate, ISO 8583 parse, LEI validate, IBAN validate.
- Both precompile families are optional via Cargo features on `nexus-evm` (`zkp`, `iso-compliance`).

**Primitive Types**
- `Hash` is always constructed with `Hash::new([u8; 32])` — never `Hash::from(...)`.
- `Address` is always constructed with `Address::new([u8; 20])` — never `Address::from(...)`.

**Node Features**
`nexus-node` acts as a feature aggregator. Each optional subsystem is gated:
- `rocksdb` — persistent state storage
- `zkp` — enables `nexus-evm/zkp`
- `iso-compliance` — enables `nexus-evm/iso-compliance`
- `p2p-network` — enables `nexus-network/p2p-network`
- `ethereum-compat` — enables Ethereum-compatible address/state encoding

### Solidity Contracts

`contracts/` contains three Solidity contracts (`NexusStaking.sol`, `NexusPrivacyPool.sol`, `Iso20022Bridge.sol`) intended for deployment on the EVM layer. These are standalone — there is no Hardhat/Foundry config in the repo.

## Known Compilation Issues

When hitting serde errors on `[u8; N]`, verify the `serde` dependency includes `features = ["derive"]`. For k256 signature recovery, use `signature.recovery_id()` rather than `signature.v` in newer k256 versions. See `NEXT-STEPS-BEFORE-COMPILE` for the original compiler error notes.
