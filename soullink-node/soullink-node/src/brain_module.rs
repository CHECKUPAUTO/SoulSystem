//! BrainModule — functional brain region with grouped neurons
//! Extracted from main.rs for modularity.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::Rng;
use chrono::Timelike;
use crate::brain::{NeuronIdx, Brain, now_f64};

#[derive(Debug, Serialize, Deserialize)]
pub struct BrainModule {
    pub name: String,
    pub neuron_indices: Vec<NeuronIdx>,
    pub activation_level: f32,
    pub importance: f32,
    pub x: f32,
    pub y: f32,
    pub color: String,
    /// Schema version for dynamic field migration.
    #[serde(default)]
    pub schema_version: u64,
    /// Dynamic extension fields for self-modification.
    #[serde(default)]
    pub extensions: HashMap<String, f64>,
    /// Per-module energy tracking (fraction 0-1, mean of neuron reserves)
    #[serde(default)]
    pub energy_reserve: f32,
    /// Per-tick energy budget allocation (0-1, default 1.0)
    #[serde(default)]
    pub energy_budget: f32,
    /// Metabolic cost multiplier (evolvable, default 1.0)
    #[serde(default)]
    pub base_metabolic_mod: f32,
    /// Budget priority (0-1, higher gets energy first)
    #[serde(default)]
    pub energy_priority: f32,
    /// Local metabolic stress (0-1, rises when budget exceeded)
    #[serde(default)]
    pub local_stress: f32,
}

impl BrainModule {
    pub fn new(name: String, x: f32, y: f32, color: String) -> Self {
        let mut rng = rand::rng();
        let priority = match name.as_str() {
            "motor" | "vision" => 0.7,
            _ => 0.5,
        };
        Self {
            name,
            neuron_indices: Vec::new(),
            activation_level: rng.random_range(0.5..0.8),
            importance: rng.random_range(0.3..0.7),
            x,
            y,
            color,
            schema_version: 0,
            extensions: HashMap::new(),
            energy_reserve: 0.5,
            energy_budget: 1.0,
            base_metabolic_mod: 1.0,
            energy_priority: priority,
            local_stress: 0.0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PÉRIODE CIRCADIENNE
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct Period {
    pub name: &'static str,
    pub start: u32,
    pub end: u32,
    pub intensity: f32,
    pub mode: &'static str,
}

const PERIODS: &[Period] = &[
    Period { name: "deep_night",     start: 0,  end: 5,  intensity: 0.40, mode: "consolidate" },
    Period { name: "dawn",           start: 5,  end: 7,  intensity: 0.60, mode: "consolidate" },
    Period { name: "morning_peak",   start: 7,  end: 12, intensity: 1.00, mode: "grow"        },
    Period { name: "midday_rest",    start: 12, end: 14, intensity: 0.70, mode: "maintain"    },
    Period { name: "afternoon_peak", start: 14, end: 18, intensity: 0.95, mode: "grow"        },
    Period { name: "evening",        start: 18, end: 22, intensity: 0.80, mode: "maintain"    },
    Period { name: "night_fall",     start: 22, end: 24, intensity: 0.50, mode: "consolidate" },
];

pub(crate) fn current_period() -> &'static Period {
    let hour = chrono::Local::now().hour();
    PERIODS
        .iter()
        .find(|p| hour >= p.start && hour < p.end)
        .unwrap_or(&PERIODS[0])
}

// ═══════════════════════════════════════════════════════════════
// STATS
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub total_neurons: usize,
    pub total_synapses: usize,
    pub active_synapses: usize,
    pub firing_neurons: usize,
    pub growth_events: u64,
    pub consolidation_events: u64,
    pub learning_cycles: u64,
    pub topics_learned: usize,
    pub spikes_count: usize,
    pub pruned_synapses: u64,
    /// Total energy reserve across all neurons (avg)
    pub energy_avg: f32,
    /// Number of neurons in energy debt (energy_reserve < 0.1)
    pub energy_debt_count: usize,
    /// Cumulative energy deficit ticks
    pub energy_deficit_ticks: u64,
    /// Global energy pool fill level (0.0-1.0 normalized)
    pub energy_pool_level: f32,
    /// Total ticks since birth (monotonic)
    pub age_ticks: u64,
    /// Peak number of neurons ever reached
    pub peak_neurons: usize,
    /// Current arousal level (0-1000 scaled for display)
    pub arousal_display: u32,
    /// Current metabolic pressure (0-1000 scaled)
    pub metabolic_pressure_display: u32,
    /// Number of times the brain has died and rebooted
    pub death_count: u32,
}

// ═══════════════════════════════════════════════════════════════
// PAIN & STRESS — système d'instinct de préservation
// ═══════════════════════════════════════════════════════════════

/// Composite pain signal derived from multiple vital signs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PainSignal {
    /// Pain from low energy (0 = full energy, 1 = complete depletion)
    pub energy_pain: f32,
    /// Pain from neuron death / inactivity (0 = healthy, 1 = all dead)
    pub atrophy_pain: f32,
    /// Pain from fitness freefall (0 = stable, 1 = crashing)
    pub fitness_pain: f32,
    /// Pain from isolation (no mesh peers) (0 = connected, 1 = alone)
    pub isolation_pain: f32,
    /// Pain from metabolic pressure (0 = balanced, 1 = starvation)
    pub pressure_pain: f32,
    /// Pain from arousal dysregulation (0 = optimal, 1 = extreme)
    pub arousal_pain: f32,
    /// Composite pain: weighted sum of all sources (0–1)
    pub composite: f32,
    /// Previous fitness value for delta computation
    pub last_fitness: f64,
}

impl Default for PainSignal {
    fn default() -> Self {
        Self {
            energy_pain: 0.0,
            atrophy_pain: 0.0,
            fitness_pain: 0.0,
            isolation_pain: 0.0,
            pressure_pain: 0.0,
            arousal_pain: 0.0,
            composite: 0.0,
            last_fitness: 0.5,
        }
    }
}

impl PainSignal {
    pub fn compute(brain: &Brain) -> Self {
        let mut p = PainSignal::default();

        // Energy pain: low avg energy + high debt fraction
        if !brain.neurons.is_empty() && brain.params.metabolism_enabled {
            let avg = brain.stats.energy_avg;
            let debt_frac = brain.stats.energy_debt_count as f32 / brain.neurons.len() as f32;
            p.energy_pain = ((1.0 - avg) * 0.5 + debt_frac * 0.5).clamp(0.0, 1.0);
            // Blend with pool health: 70% neuron-level, 30% pool-level
            let pool_health = brain.stats.energy_pool_level;
            let pool_pain = (1.0 - pool_health).clamp(0.0, 1.0);
            p.energy_pain = (p.energy_pain * 0.7 + pool_pain * 0.3).clamp(0.0, 1.0);
        }

        // Atrophy pain: dead/stalled module fraction
        let total = brain.module_order.len().max(1);
        let dead = brain.module_order.iter()
            .filter(|name| brain.modules.get(*name).map(|m| m.activation_level < 0.05).unwrap_or(true))
            .count();
        p.atrophy_pain = dead as f32 / total as f32;

        // Isolation pain: no peers seen recently
        let now = now_f64();
        let alive_peers = brain.mesh.peers.values()
            .filter(|peer| peer.is_alive && (now - peer.last_seen) < 30.0)
            .count();
        p.isolation_pain = if alive_peers == 0 { 0.3 } else { (1.0 / alive_peers as f32).min(1.0) };

        // Fitness pain: chute de fitness + niveau bas chronique
        let fitness_val = brain.evolution.fitness.current.overall;
        let fitness_delta = p.last_fitness - fitness_val;
        let drop_pain = (fitness_delta * 3.0_f64).clamp(0.0, 1.0) as f32;
        let low_pain = (1.0_f64 - fitness_val).clamp(0.0, 1.0) as f32;
        p.fitness_pain = (drop_pain * 0.6 + low_pain * 0.4).clamp(0.0, 1.0);
        p.last_fitness = fitness_val;

        // Pressure pain: metabolic imbalance
        p.pressure_pain = brain.metabolic_pressure.clamp(0.0, 1.0);

        // Arousal pain: penalize extreme arousal (too high or too low)
        if brain.params.metabolism_enabled {
            let arousal_dev = (brain.arousal - 0.5).abs() * 2.0;
            p.arousal_pain = (arousal_dev * 0.3).clamp(0.0, 1.0);
        }

        // Composite — six components with adjusted weights
        p.composite = (p.energy_pain * 0.35 + p.atrophy_pain * 0.20
            + p.isolation_pain * 0.05 + p.fitness_pain * 0.15
            + p.pressure_pain * 0.15 + p.arousal_pain * 0.10)
            .clamp(0.0, 1.0);

        p
    }
}

/// Stress response mode triggered by pain level.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum StressMode {
    #[default]
    /// Normal operation
    Calm,
    /// Mild stress: conserve energy, reduce growth
    Alert,
    /// Moderate stress: prioritize repair over growth
    Stress,
    /// High stress: emergency consolidation, metabolic shutdown
    Panic,
}

impl StressMode {
    pub fn from_pain(pain: f32) -> Self {
        if pain >= 0.7 {
            StressMode::Panic
        } else if pain >= 0.4 {
            StressMode::Stress
        } else if pain >= 0.2 {
            StressMode::Alert
        } else {
            StressMode::Calm
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SPIKE INFO (pour visualisation)
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpikeInfo {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub time: f64,
}

// ═══════════════════════════════════════════════════════════════
// MESH STATE — gossip protocol
// ═══════════════════════════════════════════════════════════════

/// Per-peer metadata tracked by the gossip protocol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshPeerInfo {
    pub name: String,
    pub last_seen: f64,
    pub generation: u64,
    pub fitness: f64,
    pub frontier_hypervolume: f64,
    pub is_alive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshState {
    pub peers: HashMap<u16, MeshPeerInfo>,
    /// Seed ports for initial discovery
    pub seed_ports: Vec<u16>,
}

impl Default for MeshState {
    fn default() -> Self {
        Self {
            peers: HashMap::new(),
            seed_ports: vec![9010, 9011, 9012, 9013, 9014, 9015],
        }
    }
}

/// Gossip message propagated through the mesh with TTL loop prevention.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GossipMessage {
    /// "heartbeat", "evolution", "peer_discovery"
    pub msg_type: String,
    pub source_port: u16,
    pub source_name: String,
    /// Time-to-live — decremented at each hop, dropped at 0
    pub ttl: u8,
    pub payload: serde_json::Value,
}

