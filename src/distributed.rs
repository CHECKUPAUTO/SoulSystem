//! Distributed islands — cross-host coordination scaffolding.
//!
//! Status: **scaffold**. The trait-based abstraction is final; the only
//! backend ready today is the in-memory `LocalBus`. A NATS or Redis-Streams
//! adapter will land once the network broker is selected on Monday (Thor +
//! T430 link).
//!
//! Design:
//!   - Every host runs one `EvolutionEngine` with N local islands.
//!   - Migrations between hosts go through a `MessageBus` (trait).
//!   - Each host publishes its elite migrants periodically and subscribes to
//!     migrants from peers, injecting them into local islands.
//!   - The bus is **at-least-once** and **partition-tolerant** — duplicates
//!     are accepted (the local population is bounded so they get evicted).
//!
//! To plug a real broker, implement `MessageBus` and pass it via
//! `EvolutionEngine::with_bus(Arc::new(your_bus))` (not yet wired into the
//! main loop, will land Monday).

use crate::program::Program;
use anyhow::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::{debug, info};

/// One migration event carried by the bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migrant {
    /// Originating host identifier (e.g. `"t430"`, `"thor"`).
    pub host: String,
    /// Run identifier so a receiver can correlate.
    pub run_id: String,
    /// Local island index on the *source* host.
    pub source_island: usize,
    pub program: Program,
    /// Best score on the source island when this migrant was published.
    pub source_best: f64,
    /// Wallclock of the source host at publish time.
    pub ts: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait MessageBus: Send + Sync {
    /// Publish a migrant to all subscribers.
    async fn publish(&self, m: &Migrant) -> Result<()>;

    /// Drain pending migrants without blocking.
    async fn drain(&self, max: usize) -> Vec<Migrant>;

    /// Healthcheck used by `/health` and the dashboard.
    async fn health(&self) -> bool;

    /// Implementation name for logging.
    fn name(&self) -> &str;
}

// ── LocalBus — in-process backend, useful for tests + single-host runs ──

#[derive(Default)]
pub struct LocalBus {
    queue: Mutex<VecDeque<Migrant>>,
    capacity: usize,
}

impl LocalBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
            capacity: capacity.max(1),
        }
    }
    pub fn shared(capacity: usize) -> Arc<Self> {
        Arc::new(Self::new(capacity))
    }

    pub fn pending(&self) -> usize {
        self.queue.lock().len()
    }
}

#[async_trait]
impl MessageBus for LocalBus {
    async fn publish(&self, m: &Migrant) -> Result<()> {
        let mut q = self.queue.lock();
        if q.len() >= self.capacity {
            q.pop_front();
        }
        q.push_back(m.clone());
        debug!(host=%m.host, score=m.program.score, "LocalBus publish");
        Ok(())
    }
    async fn drain(&self, max: usize) -> Vec<Migrant> {
        let mut q = self.queue.lock();
        let n = q.len().min(max);
        q.drain(..n).collect()
    }
    async fn health(&self) -> bool {
        true
    }
    fn name(&self) -> &str {
        "local"
    }
}

// ── Migration manager ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionConfig {
    /// Host identifier published with every migrant.
    pub host_id: String,
    /// Send a migrant every N iterations. 0 = never publish.
    pub publish_interval: u32,
    /// Drain inbound migrants every N iterations. 0 = never read.
    pub poll_interval: u32,
    /// Max migrants to inject per poll.
    pub max_inject: usize,
}

impl Default for DistributionConfig {
    fn default() -> Self {
        let host_id = hostname_best_effort();
        Self {
            host_id,
            publish_interval: 0, // disabled by default
            poll_interval: 0,
            max_inject: 4,
        }
    }
}

fn hostname_best_effort() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "local".into())
}

/// High-level wrapper used by `EvolutionEngine`.
pub struct DistributionManager {
    cfg: DistributionConfig,
    bus: Arc<dyn MessageBus>,
}

impl DistributionManager {
    pub fn new(cfg: DistributionConfig, bus: Arc<dyn MessageBus>) -> Self {
        info!(
            host = %cfg.host_id,
            bus = bus.name(),
            "distribution manager ready"
        );
        Self { cfg, bus }
    }

    /// Whether outbound publishing is currently active.
    pub fn publishes(&self) -> bool {
        self.cfg.publish_interval > 0
    }

    /// Whether inbound polling is currently active.
    pub fn polls(&self) -> bool {
        self.cfg.poll_interval > 0
    }

    pub async fn maybe_publish(
        &self,
        iter: u32,
        run_id: &str,
        program: &Program,
        source_island: usize,
        source_best: f64,
    ) -> Result<()> {
        if self.cfg.publish_interval == 0 || iter % self.cfg.publish_interval != 0 {
            return Ok(());
        }
        let m = Migrant {
            host: self.cfg.host_id.clone(),
            run_id: run_id.to_string(),
            source_island,
            program: program.clone(),
            source_best,
            ts: chrono::Utc::now(),
        };
        self.bus.publish(&m).await
    }

    pub async fn maybe_poll(&self, iter: u32) -> Vec<Migrant> {
        if self.cfg.poll_interval == 0 || iter % self.cfg.poll_interval != 0 {
            return Vec::new();
        }
        let migrants = self.bus.drain(self.cfg.max_inject).await;
        // Filter out our own host's migrants — published and immediately echoed.
        migrants
            .into_iter()
            .filter(|m| m.host != self.cfg.host_id)
            .collect()
    }

    pub fn bus_name(&self) -> &str {
        self.bus.name()
    }

    pub async fn bus_healthy(&self) -> bool {
        self.bus.health().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_migrant() -> Migrant {
        let mut p = Program::new("fn x(){}".into(), 0, None, 0);
        p.score = 0.42;
        Migrant {
            host: "t430".into(),
            run_id: "r1".into(),
            source_island: 0,
            program: p,
            source_best: 0.42,
            ts: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn local_bus_publish_and_drain() {
        let bus = LocalBus::new(10);
        for _ in 0..3 {
            bus.publish(&sample_migrant()).await.unwrap();
        }
        let drained = bus.drain(10).await;
        assert_eq!(drained.len(), 3);
        assert_eq!(bus.pending(), 0);
    }

    #[tokio::test]
    async fn local_bus_capacity_evicts() {
        let bus = LocalBus::new(2);
        for _ in 0..5 {
            bus.publish(&sample_migrant()).await.unwrap();
        }
        assert!(bus.pending() <= 2);
    }

    #[tokio::test]
    async fn manager_publish_respects_interval() {
        let bus: Arc<dyn MessageBus> = Arc::new(LocalBus::new(10));
        let mgr = DistributionManager::new(
            DistributionConfig {
                host_id: "h1".into(),
                publish_interval: 5,
                poll_interval: 5,
                max_inject: 4,
            },
            Arc::clone(&bus),
        );
        let mut p = Program::new("x".into(), 0, None, 0);
        p.score = 0.1;
        // iter 1..=4 → no publish
        for i in 1..=4u32 {
            mgr.maybe_publish(i, "r", &p, 0, 0.1).await.unwrap();
        }
        // We can't directly inspect bus from manager but we can cast via the
        // concrete type we control:
        // Easier: do a drain on the same bus pointer
        let pre = bus.drain(10).await;
        assert!(pre.is_empty(), "no publish before interval");
        // iter 5 → publish
        mgr.maybe_publish(5, "r", &p, 0, 0.1).await.unwrap();
        let post = bus.drain(10).await;
        assert_eq!(post.len(), 1);
        assert_eq!(post[0].host, "h1");
    }

    #[tokio::test]
    async fn poll_filters_own_host() {
        let bus: Arc<dyn MessageBus> = Arc::new(LocalBus::new(10));
        let mgr = DistributionManager::new(
            DistributionConfig {
                host_id: "self".into(),
                publish_interval: 1,
                poll_interval: 1,
                max_inject: 10,
            },
            Arc::clone(&bus),
        );
        let mut p = Program::new("x".into(), 0, None, 0);
        p.score = 0.5;
        mgr.maybe_publish(1, "r", &p, 0, 0.5).await.unwrap();
        let mut peer = sample_migrant();
        peer.host = "peer".into();
        bus.publish(&peer).await.unwrap();

        let drained = mgr.maybe_poll(1).await;
        // Self echo filtered out → only the peer remains
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].host, "peer");
    }
}
