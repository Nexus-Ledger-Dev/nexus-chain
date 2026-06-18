//! libp2p network service — Swarm event loop, gated by the `p2p-network` feature.

#[cfg(feature = "p2p-network")]
pub use inner::*;

#[cfg(feature = "p2p-network")]
mod inner {
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tracing::{debug, info, warn};
    use futures::StreamExt;

    use libp2p::{
        gossipsub, identify, kad,
        kad::store::MemoryStore,
        multiaddr::Protocol,
        noise, tcp, yamux,
        swarm::SwarmEvent,
        Multiaddr, PeerId, SwarmBuilder,
    };
    use libp2p::swarm::NetworkBehaviour;

    /// Parse a `/ip4/.../tcp/.../p2p/<PeerID>` string into a `(PeerId, Multiaddr)` pair.
    fn parse_bootstrap_addr(s: &str) -> Option<(PeerId, Multiaddr)> {
        let mut addr: Multiaddr = s.parse().ok()?;
        let peer_id = match addr.pop()? {
            Protocol::P2p(id) => id,
            _ => return None,
        };
        Some((peer_id, addr))
    }

    use crate::{
        GossipConfig, GossipHandler, NetworkError, NetworkMessage,
        NetworkResult, PeerInfo, PeerManager, PeerState,
    };

    // ─── command channel ─────────────────────────────────────────────────────

    /// Commands accepted by the running network service.
    pub enum NetworkCommand {
        /// Publish raw bytes on a named gossipsub topic.
        Publish { topic: String, data: Vec<u8> },
        /// Graceful shutdown.
        Shutdown,
    }

    /// Cloneable handle returned by `start_network`.
    #[derive(Clone)]
    pub struct NetworkHandle {
        cmd_tx: mpsc::Sender<NetworkCommand>,
    }

    impl NetworkHandle {
        pub async fn publish(&self, topic: &str, data: Vec<u8>) -> NetworkResult<()> {
            self.cmd_tx
                .send(NetworkCommand::Publish { topic: topic.to_string(), data })
                .await
                .map_err(|_| NetworkError::ChannelClosed)
        }

        pub async fn shutdown(&self) {
            let _ = self.cmd_tx.send(NetworkCommand::Shutdown).await;
        }
    }

    // ─── combined behaviour ───────────────────────────────────────────────────

    /// libp2p network behaviour combining gossipsub + Kademlia DHT + identify.
    #[derive(NetworkBehaviour)]
    pub struct NexusBehaviour {
        pub gossipsub: gossipsub::Behaviour,
        pub kademlia: kad::Behaviour<MemoryStore>,
        pub identify: identify::Behaviour,
    }

    // ─── configuration ───────────────────────────────────────────────────────

    /// Network service configuration.
    pub struct NetworkConfig {
        /// TCP listen address.
        pub listen_addr: Multiaddr,
        /// Bootstrap peer multiaddrs in the form `/ip4/.../tcp/.../p2p/<PeerID>`.
        /// Parsed inside `run_swarm` so callers don't need libp2p as a direct dep.
        pub bootstrap_nodes: Vec<String>,
        /// Maximum gossipsub message size in bytes.
        pub max_message_size: usize,
    }

    impl Default for NetworkConfig {
        fn default() -> Self {
            Self {
                listen_addr: "/ip4/0.0.0.0/tcp/30333".parse().expect("valid multiaddr"),
                bootstrap_nodes: vec![],
                max_message_size: 1024 * 1024,
            }
        }
    }

    /// Named gossipsub topics for NexusChain protocol messages.
    pub const TOPIC_TRANSACTIONS: &str = "nexus/transactions/1.0";
    pub const TOPIC_VERTICES: &str     = "nexus/vertices/1.0";
    pub const TOPIC_VOTES: &str        = "nexus/votes/1.0";

    // ─── public API ──────────────────────────────────────────────────────────

    /// Spawn the libp2p Swarm event loop in the background.
    ///
    /// Returns a `NetworkHandle` that can be used to publish messages and
    /// request graceful shutdown.
    pub async fn start_network(
        config: NetworkConfig,
        peer_manager: Arc<PeerManager>,
        message_tx: mpsc::Sender<NetworkMessage>,
    ) -> NetworkResult<NetworkHandle> {
        let (cmd_tx, cmd_rx) = mpsc::channel(256);
        let gossip_handler = GossipHandler::new(GossipConfig::default(), message_tx);

        tokio::spawn(async move {
            if let Err(e) = run_swarm(config, peer_manager, gossip_handler, cmd_rx).await {
                warn!("Network Swarm exited with error: {:?}", e);
            }
        });

        Ok(NetworkHandle { cmd_tx })
    }

    // ─── swarm loop ──────────────────────────────────────────────────────────

    async fn run_swarm(
        config: NetworkConfig,
        peer_manager: Arc<PeerManager>,
        gossip_handler: GossipHandler,
        mut cmd_rx: mpsc::Receiver<NetworkCommand>,
    ) -> NetworkResult<()> {
        let max_message_size  = config.max_message_size;
        let listen_addr       = config.listen_addr;
        let bootstrap_nodes   = config.bootstrap_nodes;

        let mut swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?
            .with_behaviour(|key| -> Result<_, Box<dyn std::error::Error + Send + Sync>> {
                let gsub_config = gossipsub::ConfigBuilder::default()
                    .max_transmit_size(max_message_size)
                    .build()
                    .map_err(|s| -> Box<dyn std::error::Error + Send + Sync> {
                        s.to_string().into()
                    })?;

                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gsub_config,
                ).map_err(|s| -> Box<dyn std::error::Error + Send + Sync> {
                    s.to_string().into()
                })?;

                let local_peer_id = key.public().to_peer_id();
                let kademlia = kad::Behaviour::new(
                    local_peer_id,
                    MemoryStore::new(local_peer_id),
                );

                let identify = identify::Behaviour::new(
                    identify::Config::new("/nexus/1.0.0".to_string(), key.public()),
                );

                Ok(NexusBehaviour { gossipsub, kademlia, identify })
            })
            .map_err(|e| NetworkError::ProtocolError(e.to_string()))?
            .build();

        // Subscribe to all NexusChain topics before accepting connections.
        for topic_name in &[TOPIC_TRANSACTIONS, TOPIC_VERTICES, TOPIC_VOTES] {
            let topic = gossipsub::IdentTopic::new(*topic_name);
            swarm.behaviour_mut().gossipsub
                .subscribe(&topic)
                .map_err(|e| NetworkError::ProtocolError(e.to_string()))?;
        }

        // Start the TCP listener.
        swarm.listen_on(listen_addr)
            .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?;

        // Parse and seed Kademlia with bootstrap peers.
        for s in &bootstrap_nodes {
            if let Some((peer_id, addr)) = parse_bootstrap_addr(s) {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                swarm.dial(peer_id).ok();
            } else {
                warn!("Ignoring invalid bootstrap address: {}", s);
            }
        }

        info!("Network service running");

        loop {
            tokio::select! {
                // Process the next Swarm event.
                event = swarm.next() => {
                    let Some(event) = event else { break; };
                    on_swarm_event(event, &peer_manager, &gossip_handler).await;
                }

                // Process commands from the rest of the node.
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(NetworkCommand::Publish { topic, data }) => {
                            let t = gossipsub::IdentTopic::new(&topic);
                            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(t, data) {
                                debug!("Gossipsub publish error: {:?}", e);
                            }
                        }
                        Some(NetworkCommand::Shutdown) | None => {
                            info!("Network service shutdown requested");
                            break;
                        }
                    }
                }
            }
        }

        info!("Network service stopped");
        Ok(())
    }

    // ─── event handler ───────────────────────────────────────────────────────

    async fn on_swarm_event(
        event: SwarmEvent<NexusBehaviourEvent>,
        peer_manager: &Arc<PeerManager>,
        gossip_handler: &GossipHandler,
    ) {
        match event {
            // Incoming gossipsub message — forward to handler for dedup + dispatch.
            SwarmEvent::Behaviour(NexusBehaviourEvent::Gossipsub(
                gossipsub::Event::Message { message, .. }
            )) => {
                if let Err(e) = gossip_handler.handle_message(&message.data).await {
                    debug!("Gossip message error: {:?}", e);
                }
            }

            // New inbound or outbound connection — register peer.
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                peer_manager.add_peer(PeerInfo {
                    id: peer_id.to_string(),
                    addresses: vec![],
                    protocol_version: 1,
                    agent: String::new(),
                    is_validator: false,
                    validator_address: None,
                    last_seen: now,
                    state: PeerState::Connected,
                    reputation: 0,
                });
                info!("Peer connected: {}", peer_id);
            }

            // Connection dropped — mark disconnected (don't remove; keep history).
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                peer_manager.set_state(&peer_id.to_string(), PeerState::Disconnected);
                debug!("Peer disconnected: {}", peer_id);
            }

            // Successful bind — log the actual listen address.
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Network listening on: {}", address);
            }

            // Identify protocol — update agent string and known addresses.
            SwarmEvent::Behaviour(NexusBehaviourEvent::Identify(
                identify::Event::Received { peer_id, info }
            )) => {
                for addr in info.listen_addrs {
                    peer_manager.add_peer(PeerInfo {
                        id: peer_id.to_string(),
                        addresses: vec![addr.to_string()],
                        protocol_version: 1,
                        agent: info.agent_version.clone(),
                        is_validator: false,
                        validator_address: None,
                        last_seen: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                        state: PeerState::Connected,
                        reputation: 0,
                    });
                }
            }

            _ => {}
        }
    }
}
