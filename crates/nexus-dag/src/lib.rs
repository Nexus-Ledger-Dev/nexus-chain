//! # NexusChain DAG Module
//!
//! This module implements the Directed Acyclic Graph (DAG) data structure
//! that forms the backbone of NexusChain's consensus mechanism.
//!
//! ## Key Concepts
//!
//! - **Vertex**: A unit containing transactions, references to parent vertices
//! - **Tips**: Vertices with no children (current frontier of the DAG)
//! - **Weight**: Accumulated confidence based on descendants
//! - **Finality**: BFT overlay for deterministic finalization
//!
//! ## Advantages over Linear Blockchain
//!
//! - Parallel transaction processing
//! - Higher throughput (no sequential block production)
//! - Natural transaction ordering via causal dependencies
//! - Reduced confirmation latency

mod vertex;
mod dag;
mod tip_selection;
mod finality;
mod metrics;

pub use vertex::*;
pub use dag::*;
pub use tip_selection::*;
pub use finality::*;
pub use metrics::*;
