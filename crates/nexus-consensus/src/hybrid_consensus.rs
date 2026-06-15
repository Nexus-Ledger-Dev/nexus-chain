//! Hybrid consensus module – combines weighted PoS leader election with BFT finality
//! This module is compiled only when the `hybrid_consensus` Cargo feature is enabled.

#[cfg(feature = "hybrid_consensus")]
mod inner {
    use std::sync::Arc;
    use parking_lot::RwLock;
    use dashmap::DashMap;
    use rand::{thread_rng, Rng};
    use rand::distributions::WeightedIndex;
    use uuid::Uuid;
    use tracing::info;
    use nexus_primitives::{Address, Hash};
    use crate::{BftConsensus, ValidatorSet};
    use crate::bft::BftMessage;
    use crate::bft::Proposal;
    use crate::bft::CommitVote;
    use crate::bft::PrepareVote;
    use crate::bft::BftRoundState;
    #[cfg(feature = "hybrid_consensus")]
    use nexus_iso::{IsoResult, IsoLogEntry, Event};

    /// Represents the hybrid consensus engine.
    pub struct HybridConsensus {
        /// Underlying BFT engine for finality.
        bft: BftConsensus,
        /// Current epoch counter (incremented per block interval).
        epoch: u64,
        /// Mapping of validator address to its stake weight.
        stakes: Arc<DashMap<Address, u64>>,
    }

    impl HybridConsensus {
        /// Construct a new hybrid consensus instance.
        /// `stakes` should contain the active validator set and their staking amounts.
        pub fn new(bft: BftConsensus, stakes: Arc<DashMap<Address, u64>>) -> Self {
            Self {
                bft,
                epoch: 0,
                stakes,
            }
        }

        /// Choose a leader for the current epoch using weighted random selection.
        pub fn elect_leader(&self) -> Option<Address> {
            let addresses: Vec<Address> = self.stakes.iter().map(|e| e.key().clone()).collect();
            let weights: Vec<u64> = self.stakes.iter().map(|e| *e.value()).collect();
            if addresses.is_empty() {
                return None;
            }
            let dist = WeightedIndex::new(&weights).ok()?;
            let mut rng = thread_rng();
            let idx = dist.sample(&mut rng);
            Some(addresses[idx].clone())
        }

        /// Called when a new proposal is observed – logs ISO‑compliant entry and forwards to BFT.
        pub async fn on_proposal(&mut self, proposal: Proposal) -> Result<(), Box<dyn std::error::Error>> {
            // Emit ISO log entry if feature enabled
            #[cfg(feature = "hybrid_consensus")]
            {
                let log = IsoLogEntry {
                    id: Uuid::new_v4(),
                    timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                    event: Event::Proposal {
                        proposer: proposal.proposer.clone(),
                        epoch: proposal.epoch,
                        view: proposal.view,
                        vertex_hash: proposal.vertex_hash.clone(),
                    },
                };
                // In a real system we would route this to the ISO logger; here we just debug‑log.
                info!("ISO log entry: {:?}", log);
            }
            // Forward to underlying BFT engine (synchronous for now)
            self.bft.on_proposal(proposal)?;
            Ok(())
        }

        /// Advance epoch – typically called after a block finalization.
        pub fn advance_epoch(&mut self) {
            self.epoch = self.epoch.wrapping_add(1);
            // Could rotate stakes or recalculate weights here.
        }
    }
}

#[cfg(feature = "hybrid_consensus")]
pub use inner::HybridConsensus;
