//! SoulLink Memory — concept graph, semantic search, temporal decay.
//!
//! Core types:
//! * [`Concept`] / [`ConceptKind`] — what a node represents
//! * [`MemoryGraph`] — graph + vector store + sled persistence
//! * [`MemoryManager`] — high-level API (text → embedding → storage)
//! * [`DecayConfig`] — temporal edge-weight decay
//! * [`meditation_cycle`] — background maintenance (decay + compact + flush)

pub mod concept;
pub mod decay;
pub mod graph;
pub mod manager;
pub mod ollama_client;
pub mod persistence;
pub mod random_walk;

pub use concept::{Concept, ConceptKind};
pub use decay::DecayConfig;
pub use graph::MemoryGraph;
pub use manager::MemoryManager;
pub use ollama_client::OllamaEmbeddingClient;

use std::sync::Arc;
use tracing::{info, instrument};

/// Initialize Rayon thread pool for dual-Xeon systems.
/// Call once at startup before any vector operations.
/// Uses all available cores (64 threads on dual Xeon 32c).
pub fn init_thread_pool() {
    let cores = num_cpus::get();
    rayon::ThreadPoolBuilder::new()
        .num_threads(cores)
        .build_global()
        .expect("Failed to initialize Rayon thread pool");
    info!("Rayon thread pool initialized: {} threads", cores);
}

/// Background meditation cycle for the MemoryGraph.
///
/// Runs periodically to:
/// 1. Apply temporal decay to all edges
/// 2. Prune edges below minimum weight
/// 3. Flush sled to disk (crash-proof persistence)
///
/// Designed to be called from a tokio::spawn loop or cron.
#[instrument(skip(graph))]
pub async fn meditation_cycle(graph: Arc<MemoryGraph>) {
    // 1. Decay: reduce edge weights based on age
    if let Err(e) = graph.decay_all() {
        tracing::error!("meditation decay failed: {}", e);
    }

    // 2. Persist: flush sled to disk
    if let Err(e) = graph.persist() {
        tracing::error!("meditation persist failed: {}", e);
    }

    info!("meditation cycle complete");
}