//! Autonomy Pulse — 1Hz Hamiltonian evolution loop.
//!
//! The heartbeat of the mesh. Every second, each BrainNode evolves
//! one timestep. The pulse reads GPU temperature via the AfferentNerve
//! and injects it as thermal noise (turbulence).

use crate::afferent::AfferentNerve;
use crate::node::{Attractor, BrainNode, NodeId};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{info, instrument, warn};

/// Snapshot of the entire mesh state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSnapshot {
    pub nodes: HashMap<String, NodeSnapshot>,
    pub total_energy: f64,
    pub dominant_attractor: Attractor,
    pub avg_turbulence: f64,
    pub tick: u64,
    pub elapsed_ms: u64,
}

/// Snapshot of a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSnapshot {
    pub position: f64,
    pub momentum: f64,
    pub energy: f64,
    pub attractor: Attractor,
    pub turbulence: f64,
    pub tick: u64,
}

/// The Autonomy Pulse — evolves all brain nodes at 1Hz.
pub struct AutonomyPulse {
    nodes: Arc<Mutex<HashMap<NodeId, BrainNode>>>,
    afferent: Arc<AfferentNerve>,
    tick: AtomicU64,
    started_at: Instant,
}

use std::sync::atomic::{AtomicU64, Ordering};

impl AutonomyPulse {
    pub fn new(afferent: Arc<AfferentNerve>) -> Self {
        let mut nodes = HashMap::new();
        for id in NodeId::all() {
            nodes.insert(id, BrainNode::new(id));
        }

        Self {
            nodes: Arc::new(Mutex::new(nodes)),
            afferent,
            tick: AtomicU64::new(0),
            started_at: Instant::now(),
        }
    }

    /// Run the pulse loop. Evolves all nodes every `interval_ms` milliseconds.
    pub async fn run(&self, interval_ms: u64) {
        let mut ticker = interval(Duration::from_millis(interval_ms));
        info!("AutonomyPulse: starting at {}ms interval", interval_ms);

        loop {
            ticker.tick().await;
            self.step().await;
        }
    }

    /// Single evolution step for all nodes.
    pub async fn step(&self) {
        let thermal = self.afferent.gpu_thermal_noise().await;
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);

        let mut nodes = self.nodes.lock();
        for node in nodes.values_mut() {
            // Small random perturbation (cognitive noise)
            let noise = rand::random::<f64>() * 0.001;
            node.evolve(0.0, thermal + noise);
        }

        if tick % 60 == 0 {
            // Log every 60 ticks (~1 minute)
            let snapshot = self.snapshot_internal(&nodes);
            info!(
                "Pulse tick {}: energy={:.4} turbulence={:.4} attractor={:?}",
                tick, snapshot.total_energy, snapshot.avg_turbulence, snapshot.dominant_attractor
            );
        }
    }

    /// Inject a stimulus into a specific node.
    pub fn surge(&self, node_id: NodeId, force: f64) {
        let mut nodes = self.nodes.lock();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.surge(force);
            info!("Surge: {:?} +{:.3}", node_id, force);
        }
    }

    /// Get a snapshot of the current mesh state.
    pub fn snapshot(&self) -> MeshSnapshot {
        let nodes = self.nodes.lock();
        self.snapshot_internal(&nodes)
    }

    fn snapshot_internal(&self, nodes: &HashMap<NodeId, BrainNode>) -> MeshSnapshot {
        let mut total_energy = 0.0;
        let mut total_turbulence = 0.0;
        let mut attractor_counts: HashMap<Attractor, usize> = HashMap::new();

        let node_snapshots: HashMap<String, NodeSnapshot> = nodes
            .iter()
            .map(|(id, node)| {
                let energy = node.energy();
                total_energy += energy;
                total_turbulence += node.turbulence;
                let attractor = node.attractor();
                *attractor_counts.entry(attractor).or_insert(0) += 1;

                (
                    format!("{:?}", id).to_lowercase(),
                    NodeSnapshot {
                        position: node.position,
                        momentum: node.momentum,
                        energy,
                        attractor,
                        turbulence: node.turbulence,
                        tick: node.tick,
                    },
                )
            })
            .collect();

        let dominant = attractor_counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(a, _)| a)
            .unwrap_or(Attractor::Transient);

        let n = nodes.len().max(1) as f64;

        MeshSnapshot {
            nodes: node_snapshots,
            total_energy,
            dominant_attractor: dominant,
            avg_turbulence: total_turbulence / n,
            tick: self.tick.load(Ordering::Relaxed),
            elapsed_ms: self.started_at.elapsed().as_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_creates_all_nodes() {
        let afferent = Arc::new(AfferentNerve::new("http://127.0.0.1:9020"));
        let pulse = AutonomyPulse::new(afferent);
        let snap = pulse.snapshot();
        assert_eq!(snap.nodes.len(), NodeId::all().len());
    }

    #[test]
    fn surge_modifies_node() {
        let afferent = Arc::new(AfferentNerve::new("http://127.0.0.1:9020"));
        let pulse = AutonomyPulse::new(afferent);
        pulse.surge(NodeId::Reasoning, 0.5);
        let snap = pulse.snapshot();
        let reasoning = snap.nodes.get("reasoning").unwrap();
        assert!((reasoning.momentum - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn single_step_evolution() {
        let afferent = Arc::new(AfferentNerve::new("http://127.0.0.1:9020"));
        let pulse = AutonomyPulse::new(afferent);
        pulse.step().await;
        let tick = pulse.tick.load(Ordering::Relaxed);
        assert_eq!(tick, 1);
    }

    #[test]
    fn snapshot_has_dominant_attractor() {
        let afferent = Arc::new(AfferentNerve::new("http://127.0.0.1:9020"));
        let pulse = AutonomyPulse::new(afferent);
        let snap = pulse.snapshot();
        // At rest, all nodes should be in DeepBasin
        assert!(matches!(snap.dominant_attractor, Attractor::DeepBasin));
    }
}
