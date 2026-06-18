# NexusChain

A next-generation Layer 1 blockchain built with:
- **DAG (Directed Acyclic Graph)** consensus for parallel transaction processing
- **EVM Compatibility** for Ethereum smart contract support
- **Zero-Knowledge Proofs (ZKP)** for privacy-preserving transactions
- **ISO 20022/8583 Compliance** for financial services interoperability
- **Modular Architecture** for easy upgrades and customization

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        NexusChain Node                         │
├─────────────────────────────────────────────────────────────────┤
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────────┐   │
│  │   JSON-RPC    │  │   P2P Net     │  │    WebSocket      │   │
│  │   (Eth API)   │  │   (libp2p)    │  │    (Events)       │   │
│  └───────────────┘  └───────────────┘  └───────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Consensus Layer                       │   │
│  │  ┌─────────┐  ┌──────────┐  ┌─────────────────────────┐ │   │
│  │  │   BFT   │  │   PoS    │  │     Tip Selection       │ │   │
│  │  │ Finality│  │  Staking │  │        (MCMC)           │ │   │
│  │  └─────────┘  └──────────┘  └─────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                      DAG Core                            │   │
│  │  ┌─────────┐  ┌──────────┐  ┌─────────────────────────┐ │   │
│  │  │ Vertices│  │  Parents │  │       Finality          │ │   │
│  │  │ Storage │  │  Tracking│  │       Tracking          │ │   │
│  │  └─────────┘  └──────────┘  └─────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐    │
│  │   EVM    │  │   ZKP    │  │   ISO    │  │    State     │    │
│  │ Executor │  │ Circuits │  │ Routing  │  │    Trie      │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Key Features

### 1. DAG-Based Consensus
- **Parallel Processing**: Multiple transactions can be confirmed simultaneously
- **High Throughput**: No sequential block limitation
- **MCMC Tip Selection**: Weighted random walk for fair transaction ordering
- **BFT Finality Gadget**: Deterministic finality overlay on probabilistic DAG

### 2. EVM Compatibility
- **Full EVM Support**: Deploy existing Ethereum contracts
- **Custom Precompiles**: ZKP verification, ISO message parsing, DAG queries
- **EIP-1559 Gas Model**: Predictable transaction fees
- **Solidity Compatible**: Standard development tools work out of the box

### 3. Zero-Knowledge Proofs
- **Groth16 Proving System**: Efficient proof generation and verification
- **Private Transfers**: Shield transaction amounts and participants
- **Compliance Proofs**: Prove regulatory compliance without revealing details
- **Balance Proofs**: Prove sufficient funds without exposing exact balance

### 4. ISO Financial Standards
- **ISO 20022**: XML-based payment messages (pacs.008, pacs.002)
- **ISO 8583**: Card transaction messaging
- **ISO 4217**: Currency codes with proper decimals
- **Validators**: IBAN, BIC, LEI validation

## Module Structure

```
nexus-chain/
├── crates/
│   ├── nexus-primitives/    # Core types (Hash, Address, Transaction)
│   ├── nexus-dag/           # DAG structure and tip selection
│   ├── nexus-consensus/     # PoS, BFT, validator management
│   ├── nexus-evm/           # EVM execution, precompiles
│   ├── nexus-zkp/           # ZK circuits, Groth16 prover/verifier
│   ├── nexus-iso/           # ISO 20022/8583, validators
│   ├── nexus-state/         # Merkle Patricia Trie, storage
│   ├── nexus-network/       # P2P networking, gossip, sync
│   ├── nexus-rpc/           # JSON-RPC API (Eth + Nexus)
│   └── nexus-node/          # Node runner, CLI
├── contracts/               # Solidity smart contracts
│   ├── NexusStaking.sol     # Staking/delegation
│   ├── NexusPrivacyPool.sol # ZKP privacy mixer
│   └── Iso20022Bridge.sol   # ISO message bridge
└── tests/                   # Integration tests
```

## Quick Start

### Prerequisites
- Rust 1.75+
- Cargo

### Build
```bash
cd nexus-chain
cargo build --release
```

### Run Node
```bash
# Initialize data directory
./target/release/nexus init --data-dir ~/.nexus --chain-id 1337

# Run as non-validator
./target/release/nexus run --rpc-addr 127.0.0.1:8545

# Run as validator
./target/release/nexus generate-key --output ~/.nexus/validator.key
./target/release/nexus run --validator --validator-key ~/.nexus/validator.key
```

### Configuration (config.toml)
```toml
[node]
name = "my-nexus-node"
chain_id = 1337

[network]
listen_addr = "/ip4/0.0.0.0/tcp/9000"
bootstrap_nodes = []
max_peers = 50

[rpc]
http_enabled = true
http_addr = "127.0.0.1:8545"
ws_enabled = true
ws_addr = "127.0.0.1:8546"

[consensus]
validator = false
vertex_interval_ms = 500
max_txs_per_vertex = 1000
```

## RPC API

### Ethereum Compatible Methods
- `eth_chainId`, `eth_blockNumber`, `eth_gasPrice`
- `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`
- `eth_sendRawTransaction`, `eth_call`, `eth_estimateGas`
- `eth_getBlockByNumber`, `eth_getBlockByHash`
- `web3_clientVersion`, `web3_sha3`
- `net_version`, `net_listening`, `net_peerCount`

### NexusChain Extensions
- `nexus_dagInfo` - Get DAG statistics
- `nexus_getVertex` - Get vertex by hash
- `nexus_getTips` - Get current DAG tips
- `nexus_validators` - Get validator set
- `nexus_checkCompliance` - Check ISO compliance
- `nexus_validateIban` - Validate IBAN
- `nexus_zkpVerify` - Verify ZK proof

## Smart Contract Development

### Deploying Contracts
Use standard Ethereum tools (Hardhat, Foundry, Remix) with NexusChain RPC:

```javascript
// hardhat.config.js
module.exports = {
  networks: {
    nexus: {
      url: "http://localhost:8545",
      chainId: 1337,
    }
  }
};
```

### Custom Precompiles

| Address | Name | Description |
|---------|------|-------------|
| 0x0100 | ZKP_VERIFIER | Verify Groth16 proofs |
| 0x0101 | ISO_PARSER | Parse ISO 20022 messages |
| 0x0102 | DAG_QUERY | Query DAG state |

## Modularity & Upgrades

NexusChain is designed for easy upgrades:

1. **Consensus Module**: Swap PoS for PoA, adjust finality parameters
2. **EVM Module**: Add new precompiles, update gas costs
3. **ZKP Module**: Change proving system, add new circuit types
4. **ISO Module**: Add new message types, update validation rules
5. **Network Module**: Change gossip protocol, adjust sync strategy

Each module has clean interfaces allowing independent updates without breaking others.

## Security Considerations

- **Validator Keys**: Store in HSM for production
- **ZKP Trusted Setup**: Use multi-party computation
- **ISO Compliance**: Integrate with real compliance oracles
- **Network**: Enable TLS for RPC, use noise protocol for P2P

## Contributing

1. Fork the repository
2. Create feature branch
3. Run tests: `cargo test`
4. Submit pull request

## License

Apache 2.0

## Acknowledgments

- arkworks team for ZKP libraries
- REVM for EVM implementation
- libp2p for networking
# NEXUS
