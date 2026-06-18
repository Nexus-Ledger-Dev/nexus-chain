================================================================================
NEXUSCHAIN TESTNET SETUP AND FEATURE GUIDE
================================================================================

--------------------------------------------------------------------------------
PART 1 — UBUNTU SERVER SETUP GUIDE
--------------------------------------------------------------------------------

PREREQUISITES
-------------
Hardware (per node):
  - Ubuntu 22.04 LTS or 24.04 LTS
  - 2 CPU cores, 4 GB RAM minimum
  - 20 GB disk
  - Nodes must be able to reach each other on TCP port 9000
    (or whichever P2P port you choose)


1. INSTALL BUILD DEPENDENCIES
------------------------------

    sudo apt update && sudo apt install -y \
        build-essential curl git pkg-config \
        libssl-dev libclang-dev clang

Install Rust (if not already installed):

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source "$HOME/.cargo/env"
    rustup update stable


2. CLONE AND BUILD
------------------

    git clone https://github.com/your-org/nexus-chain.git
    cd nexus-chain

    # Full production build — enables P2P networking
    cargo build --release --features p2p-network

    # The binary lands at:
    ls -lh target/release/nexus

NOTE: The first build takes 5-10 minutes due to cryptographic dependencies
(ark-bn254, openssl). Subsequent builds are fast.


3. OPEN FIREWALL PORTS
----------------------

On each server:

    sudo ufw allow 9000/tcp    # P2P (Node 1 default)
    sudo ufw allow 9001/tcp    # P2P (Node 2, if on same server)
    sudo ufw allow 8545/tcp    # RPC Node 1 (restrict to trusted IPs in production)
    sudo ufw allow 8546/tcp    # RPC Node 2
    sudo ufw enable


4. INITIALISE DATA DIRECTORIES
-------------------------------

    # Node 1
    ./target/release/nexus init --data-dir /opt/nexus/node1 --chain-id 1337

    # Node 2
    ./target/release/nexus init --data-dir /opt/nexus/node2 --chain-id 1337

This writes a config.toml into each directory. You can edit it or use CLI
flags — flags override the file.


5. (OPTIONAL) PRE-FUND ACCOUNTS WITH A GENESIS FILE
-----------------------------------------------------

Without this, every EVM transaction fails with insufficient funds.
Create /opt/nexus/genesis.json:

    {
      "chain_id": 1337,
      "name": "NexusChain Testnet",
      "timestamp": 0,
      "validators": [],
      "accounts": [
        {
          "address": "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
          "balance": "10000000000000000000000",
          "nonce": 0
        },
        {
          "address": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
          "balance": "10000000000000000000000",
          "nonce": 0
        }
      ]
    }

Import it into both nodes BEFORE starting them:

    cp /opt/nexus/genesis.json /opt/nexus/node1/genesis.json
    cp /opt/nexus/genesis.json /opt/nexus/node2/genesis.json


6. START NODE 1 (VALIDATOR)
----------------------------

    ./target/release/nexus run \
        --validator \
        --rpc-addr 0.0.0.0:8545 \
        --p2p-addr /ip4/0.0.0.0/tcp/9000 \
        --chain-id 1337 \
        --data-dir /opt/nexus/node1 \
        --log-level info

Watch the startup logs for the libp2p listener line:

    INFO nexus_network::service: Network listening on: /ip4/0.0.0.0/tcp/9000
    INFO nexus_network::service: Network service running

Query the RPC once Node 1 is up to confirm it is running:

    curl -s -X POST http://127.0.0.1:8545 \
      -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_dagInfo","params":[],"id":1}' | jq .

TIP: Run Node 1 in a screen or tmux session so you can start Node 2 in
another window on the same machine.


7. GET NODE 1'S PEER ID
------------------------

For a stable PeerID across restarts, generate a key file explicitly:

    ./target/release/nexus generate-key --output /opt/nexus/node1/validator.key

Then restart Node 1 adding:  --validator-key /opt/nexus/node1/validator.key

The PeerID is logged when Node 2 connects:

    INFO nexus_network::service: Peer connected: 12D3KooW...

You can also read it from Node 2's logs after it dials Node 1 in step 8.
For a same-LAN testnet, Kademlia discovers peers automatically without
needing an explicit PeerID — just provide the IP and port.


8. START NODE 2 (OBSERVER)
---------------------------

Replace <NODE1_PEER_ID> with the PeerID from step 7, and <NODE1_IP> with
Node 1's public or LAN IP address:

    ./target/release/nexus run \
        --rpc-addr 0.0.0.0:8546 \
        --p2p-addr /ip4/0.0.0.0/tcp/9001 \
        --bootstrap /ip4/<NODE1_IP>/tcp/9000/p2p/<NODE1_PEER_ID> \
        --chain-id 1337 \
        --data-dir /opt/nexus/node2 \
        --log-level info

You should see on Node 1's console:

    INFO nexus_network::service: Peer connected: <NODE2_PEER_ID>


9. VERIFY THE TWO-NODE SETUP
-----------------------------

    # Peer count on Node 1 (should return "0x1")
    curl -s -X POST http://127.0.0.1:8545 \
      -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}'

    # DAG growing on both nodes — vertex counts should increase together
    curl -s -X POST http://127.0.0.1:8545 \
      -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_dagInfo","params":[],"id":1}' | jq .result.total_vertices

    curl -s -X POST http://127.0.0.1:8546 \
      -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_dagInfo","params":[],"id":1}' | jq .result.total_vertices

Vertices are produced roughly every 500 ms (the default vertex_interval_ms).


10. (OPTIONAL) SYSTEMD SERVICE
--------------------------------

/etc/systemd/system/nexus-node1.service:

    [Unit]
    Description=NexusChain Node 1
    After=network.target

    [Service]
    User=ubuntu
    ExecStart=/home/ubuntu/nexus-chain/target/release/nexus run \
        --validator \
        --rpc-addr 0.0.0.0:8545 \
        --p2p-addr /ip4/0.0.0.0/tcp/9000 \
        --chain-id 1337 \
        --data-dir /opt/nexus/node1
    Restart=on-failure
    RestartSec=5

    [Install]
    WantedBy=multi-user.target

    sudo systemctl daemon-reload
    sudo systemctl enable nexus-node1
    sudo systemctl start nexus-node1
    sudo journalctl -fu nexus-node1


================================================================================
PART 2 — NEXUSCHAIN DESCRIPTION AND FEATURE TESTING GUIDE
================================================================================

WHAT IS NEXUSCHAIN?
-------------------
NexusChain is a Layer 1 blockchain designed for financial services. Its two
distinguishing characteristics are:

1. DAG-based consensus — transactions are grouped into *vertices* rather than
   linear blocks. Each vertex can reference multiple parent vertices, allowing
   parallel transaction processing and high throughput without sacrificing
   finality. BFT finality is layered on top of the DAG.

2. Financial-services precompiles — beyond the standard Ethereum precompile
   set, NexusChain adds native EVM precompiles for ISO 20022 payment message
   parsing, ZK-proof verification, and Poseidon hashing (efficient in-circuit
   hashing for ZK applications).

The EVM is Ethereum-compatible: existing Solidity contracts, tools (Hardhat,
Foundry, cast, MetaMask), and wallets work against NexusChain with chain ID
1337 (configurable).


ARCHITECTURE
------------

    +--------------------------------------------------+
    |  JSON-RPC (HTTP :8545)                           |
    |  eth_* / net_* / web3_* / nexus_*               |
    +--------------------+-----------------------------+
                         |
    +--------------------v-----------------------------+
    |  EVM Executor + StateDb                          |
    |  Precompiles: 0x01-0x09 (Ethereum standard)      |
    |               0x100-0x105 (NexusChain)           |
    +--------------------+-----------------------------+
                         |
    +--------------------v-----------------------------+
    |  DAG + BFT Consensus                             |
    |  Vertices -> finality via Snowball-style BFT     |
    +--------------------+-----------------------------+
                         |
    +--------------------v-----------------------------+
    |  P2P Network (libp2p)                            |
    |  Gossipsub + Kademlia DHT + identify             |
    +--------------------------------------------------+


IMPLEMENTATION STATUS
---------------------

Feature                                         Status
-------                                         ------
DAG consensus, BFT finality                     Fully implemented
EVM executor (transfers, deploy, call)          Fully implemented
eth_* / net_* / web3_* JSON-RPC                 Fully implemented
nexus_dagInfo/getTips/getVertex/isFinalized     Fully implemented
Ethereum precompiles 0x01-0x09                  Fully implemented
EIP-1559 and legacy RLP transaction decode      Fully implemented
P2P gossipsub + Kademlia peer discovery         Fully implemented
Poseidon hash constants (BN254)                 Implemented
IBAN / BIC validation                           Implemented
nexus_validateIban / nexus_validateBic          Implemented
Poseidon precompile (0x105)                     Interface complete *
ZKP precompiles (0x100, 0x101)                  Interface complete *
ISO 20022 precompiles (0x102, 0x103)            Interface complete *
State persistence (RocksDB)                     Not yet (state resets on restart)
WebSocket subscriptions (eth_subscribe)         Not yet

* "Interface complete" means the precompile is registered, gas is charged,
  and it returns a well-formed response. Full logic wiring is a planned next step.


FEATURE TESTING
---------------

All examples use curl. Install jq for readable output:
    sudo apt install jq

Install Foundry for cast/forge:
    curl -L https://foundry.paradigm.xyz | bash && foundryup

Set shorthands:
    N1="http://127.0.0.1:8545"
    N2="http://127.0.0.1:8546"


--- 1. BASIC ETHEREUM JSON-RPC COMPATIBILITY ---

    # Chain ID (returns 0x539 = 1337)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' | jq .result

    # Current "block" number (maps to DAG height)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq .result

    # Gas price
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_gasPrice","params":[],"id":1}' | jq .result

    # Client version
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"web3_clientVersion","params":[],"id":1}' | jq .result


--- 2. ACCOUNT STATE ---

    ADDR="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

    # Balance (in wei, hex)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$ADDR\",\"latest\"],\"id\":1}" \
      | jq .result

    # Nonce
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"$ADDR\",\"latest\"],\"id\":1}" \
      | jq .result

With Foundry:
    cast balance 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 --rpc-url $N1


--- 3. SENDING A TRANSACTION ---

The private key for 0xf39Fd6... (standard Hardhat/Foundry dev account):
    0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80

    cast send \
      --rpc-url $N1 \
      --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
      --chain-id 1337 \
      0x70997970C51812dc3A010C7d01b50e0d17dc79C8 \
      --value 1ether

Verify the balance changed:
    cast balance 0x70997970C51812dc3A010C7d01b50e0d17dc79C8 --rpc-url $N1


--- 4. DEPLOY AND CALL A CONTRACT ---

With Foundry forge:
    forge create --rpc-url $N1 \
      --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
      src/MyContract.sol:MyContract

Read-only call (no transaction):
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0xCONTRACT_ADDRESS",
        "data":"0xMETHOD_SELECTOR"
      },"latest"],"id":1}' | jq .result


--- 5. DAG-SPECIFIC RPC METHODS ---

These are unique to NexusChain — no Ethereum equivalent.

    # Overall DAG statistics
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_dagInfo","params":[],"id":1}' | jq .result
    # Returns: total_vertices, dag_height, tips_count, finalized_height

    # Current DAG tips (vertices with no children yet)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_getTips","params":[],"id":1}' | jq .result

    # Inspect a specific vertex (use a hash returned from nexus_getTips)
    HASH="0x..."
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"nexus_getVertex\",\"params\":[\"$HASH\"],\"id\":1}" \
      | jq .result

    # Check whether a vertex has been BFT-finalized
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"nexus_isFinalized\",\"params\":[\"$HASH\"],\"id\":1}" \
      | jq .result

    # Current validator set
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_validators","params":[],"id":1}' | jq .result

    # Current epoch number
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_epoch","params":[],"id":1}' | jq .result

    # Parent vertices of a given vertex
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"nexus_getParents\",\"params\":[\"$HASH\"],\"id\":1}" \
      | jq .result

    # Child vertices of a given vertex
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"nexus_getChildren\",\"params\":[\"$HASH\"],\"id\":1}" \
      | jq .result


--- 6. STANDARD ETHEREUM PRECOMPILES (0x01-0x09) ---

These behave identically to mainnet Ethereum.

ecrecover (0x01) — recover signer address from a signature:
    cast call --rpc-url $N1 0x0000000000000000000000000000000000000001 \
      "$(cast abi-encode 'f(bytes32,uint8,bytes32,bytes32)' \
         0xHASH 27 0xR 0xS)"

SHA-256 (0x02):
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000002",
        "data":"0x68656c6c6f"
      },"latest"],"id":1}' | jq .result
    # Returns SHA-256 of "hello"

RIPEMD-160 (0x03):
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000003",
        "data":"0x68656c6c6f"
      },"latest"],"id":1}' | jq .result

MODEXP (0x05) — modular exponentiation (used by RSA verification contracts):
    # Format: base_len(32 bytes) + exp_len(32 bytes) + mod_len(32 bytes) + base + exp + mod
    # Example: 2^10 mod 11 = 1
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000005",
        "data":"0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001020a0b"
      },"latest"],"id":1}' | jq .result

BN254 add (0x06), mul (0x07), pairing (0x08):
    These are tested automatically when you deploy a Groth16 or PLONK verifier
    contract generated by snarkjs or circom. Any EIP-197 pairing check will
    work correctly.

BLAKE2F (0x09) — BLAKE2b compression function (EIP-152):
    # 213-byte input: rounds(4) + h(64) + m(128) + t(16) + f(1)
    # See EIP-152 for the exact encoding.
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000009",
        "data":"0x0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001"
      },"latest"],"id":1}' | jq .result


--- 7. NEXUSCHAIN PRECOMPILES (0x100-0x105) ---

Poseidon hash (0x105) — ZK-friendly hash over BN254 scalar field:
    # Input: one or more 32-byte field elements (big-endian)
    # Single element
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000105",
        "data":"0x0000000000000000000000000000000000000000000000000000000000000001"
      },"latest"],"id":1}' | jq .result

    # Two field elements (Poseidon-2 hash)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000105",
        "data":"0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002"
      },"latest"],"id":1}' | jq .result

ZKP verify (0x100) — verify a Groth16 proof:
    # Input: proof_type(32) + num_public_inputs(32) + public_inputs(n*32) + proof_bytes
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_call","params":[{
        "to":"0x0000000000000000000000000000000000000100",
        "data":"0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000042"
      },"latest"],"id":1}' | jq .result
    # Returns 0x01 (valid) or 0x00 (invalid)

ZKP batch verify (0x101) — verify multiple proofs in one call:
    # Input: num_proofs(32) + [proof_type(32) + num_inputs(32) + inputs + proof] * n
    # Returns 0x01 only if all proofs are valid

ISO 20022 parse (0x102) — parse a payment message XML:
    # Input: UTF-8 ISO 20022 XML as raw bytes
    # Returns ABI-encoded (currency bytes3, amount uint256, iban bytes34)
    ISO_HEX=$(echo -n '<CdtTrfTxInf>...</CdtTrfTxInf>' | xxd -p | tr -d '\n')
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_call\",\"params\":[{
        \"to\":\"0x0000000000000000000000000000000000000102\",
        \"data\":\"0x$ISO_HEX\"
      },\"latest\"],\"id\":1}" | jq .result

ISO 20022 validate (0x103) — validate message structure:
    # Same input format as 0x102
    # Returns 0x01 (valid) or 0x00 (invalid)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_call\",\"params\":[{
        \"to\":\"0x0000000000000000000000000000000000000103\",
        \"data\":\"0x$ISO_HEX\"
      },\"latest\"],\"id\":1}" | jq .result


--- 8. ISO COMPLIANCE / FINANCIAL RPC METHODS ---

    # Validate an IBAN (returns true/false)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_validateIban","params":["GB82WEST12345698765432"],"id":1}' \
      | jq .result

    # Validate a BIC/SWIFT code
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_validateBic","params":["DEUTDEDB"],"id":1}' \
      | jq .result

    # Check transaction compliance against ISO 20022 rules
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_checkCompliance","params":["0xTX_HASH"],"id":1}' \
      | jq .result


--- 9. P2P NETWORK HEALTH ---

    # Peer count on each node (should be "0x1" once both are connected)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' | jq .result

    curl -s -X POST $N2 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' | jq .result

    # Network listening status
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"net_listening","params":[],"id":1}' | jq .result

    # Syncing status (false when fully synced)
    curl -s -X POST $N2 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' | jq .result


--- 10. ZKP RPC METHOD ---

    # Verify a ZK proof via the RPC layer (separate from the precompile path)
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_zkpVerify","params":[{
        "proof_type": "groth16",
        "proof": "0x...",
        "public_inputs": ["0x01", "0x02"]
      }],"id":1}' | jq .result

    # Retrieve a stored proof by hash
    curl -s -X POST $N1 -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_getProof","params":["0xPROOF_HASH"],"id":1}' | jq .result


--- 11. CLI REFERENCE ---

    nexus run          Start the node (add --validator to produce blocks)
    nexus init         Create a data directory with a default config.toml
    nexus generate-key Write a new validator keypair to a file
    nexus import-genesis  Load a genesis.json into the node's data directory
    nexus export       Dump current state as a genesis snapshot
    nexus status       Query a running node's DAG info over RPC
    nexus benchmark    Run built-in DAG / EVM benchmarks

Run any subcommand with --help for full flag details:
    nexus run --help


--- 12. COMPLETE TWO-NODE QUICK START (SAME SERVER) ---

Terminal 1 — Node 1 validator:
    RUST_LOG=info ./target/release/nexus run \
        --validator \
        --rpc-addr 127.0.0.1:8545 \
        --p2p-addr /ip4/0.0.0.0/tcp/9000 \
        --data-dir /tmp/nexus1

Terminal 2 — Get DAG info (wait 2-3 seconds for startup):
    curl -s -X POST http://127.0.0.1:8545 \
      -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","method":"nexus_dagInfo","params":[],"id":1}' | jq .

    # Note the PeerID from Terminal 1 logs, then:

Terminal 3 — Node 2 observer:
    RUST_LOG=info ./target/release/nexus run \
        --rpc-addr 127.0.0.1:8546 \
        --p2p-addr /ip4/0.0.0.0/tcp/9001 \
        --bootstrap /ip4/127.0.0.1/tcp/9000/p2p/<NODE1_PEER_ID> \
        --data-dir /tmp/nexus2

Terminal 4 — Confirm both nodes are syncing:
    watch -n 1 'curl -s -X POST http://127.0.0.1:8545 \
      -H Content-Type:application/json \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"nexus_dagInfo\",\"params\":[],\"id\":1}" | jq .result.total_vertices'

================================================================================
