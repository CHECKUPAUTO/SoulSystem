//! Shared organ infrastructure.
//!
//! Common types, turbulence calculation, and standard mesh endpoints
//! used by all 5 organs.

use axum::Router;
use dashmap::DashMap;
use serde::Deserialize;
use std::time::Instant;
use tokio::net::TcpListener;

// ── Shared state types ──────────────────────────────────────────────────────

/// Common organ configuration.
#[derive(Debug, Clone)]
pub struct OrganConfig {
    pub port: u16,
    pub n: usize,
    pub version: &'static str,
    pub name: &'static str,
}

#[derive(Deserialize)]
pub struct StimulusRequest {
    pub content: Option<String>,
    pub intensity: Option<f64>,
}

/// Core state shared by all organs (tick/spike/reward tracking).
pub struct SharedState {
    pub tick_count: DashMap<String, u64>,
    pub spikes: DashMap<String, usize>,
    pub rewards: DashMap<String, usize>,
    pub start_time: Instant,
    pub last_stimulus: DashMap<String, Instant>,
}

impl SharedState {
    pub fn new_inner() -> Self {
        Self {
            tick_count: DashMap::new(),
            spikes: DashMap::new(),
            rewards: DashMap::new(),
            start_time: Instant::now(),
            last_stimulus: DashMap::new(),
        }
    }
}

// ── Turbulence calculation ─────────────────────────────────────────────────

/// Calculate turbulence vector (value, attractor_name, is_critical).
#[inline]
pub fn calc_turbulence(spikes: &DashMap<String, usize>) -> (f64, String, bool) {
    let activity: f64 = spikes
        .iter()
        .map(|r| *r.value() as f64)
        .sum::<f64>()
        .min(100.0)
        / 100.0;
    let tv = 1.0 - activity;
    let attractor = if tv < 0.05 {
        "DeepBasin"
    } else if tv < 0.2 {
        "StableOrbit"
    } else if tv < 0.5 {
        "Transient"
    } else {
        "StrangeAttractor"
    };
    (tv.min(1.0), attractor.to_string(), tv > 0.7)
}

/// Creativity-favouring thresholds.
#[inline]
pub fn calc_turbulence_creative(spikes: &DashMap<String, usize>) -> (f64, String, bool) {
    let activity: f64 = spikes
        .iter()
        .map(|r| *r.value() as f64)
        .sum::<f64>()
        .min(100.0)
        / 100.0;
    let tv = 1.0 - activity;
    let attractor = if tv < 0.05 {
        "DeepBasin"
    } else if tv < 0.15 {
        "StableOrbit"
    } else if tv < 0.35 {
        "Transient"
    } else {
        "StrangeAttractor"
    };
    (tv.min(1.0), attractor.to_string(), tv > 0.5)
}

// ── Standard mesh endpoints ─────────────────────────────────────────────────

/// Generate the standard stats JSON response.
pub fn stats_json(
    config: &OrganConfig,
    shared: &SharedState,
    concepts_count: usize,
) -> serde_json::Value {
    let tc: u64 = shared.tick_count.iter().map(|r| *r.value()).sum();
    let sp: usize = shared.spikes.iter().map(|r| *r.value()).sum();
    let rw: usize = shared.rewards.iter().map(|r| *r.value()).sum();
    let (tv, att, crit) = calc_turbulence(&shared.spikes);
    let hz = if shared.start_time.elapsed().as_secs() > 0 {
        sp as f64 / shared.start_time.elapsed().as_secs() as f64
    } else {
        0.0
    };
    serde_json::json!({
        "N": config.n, "avg_weight": 0.5, "concepts_count": concepts_count, "hz": hz,
        "n": config.n, "name": config.name, "rewards_count": rw, "spikes": sp,
        "tick_count": tc,
        "turbulence": {
            "attractor": att, "is_critical": crit, "mean_gradient": 0.01,
            "value": tv, "variance": tv * 0.1
        },
        "version": config.version,
    })
}

/// Tick a named counter.
pub fn tick(shared: &SharedState, name: &str) {
    shared
        .tick_count
        .entry(name.to_string())
        .and_modify(|v| *v += 1)
        .or_insert(1);
}

/// Spike a named counter.
pub fn spike(shared: &SharedState, name: &str) {
    shared
        .spikes
        .entry(name.to_string())
        .and_modify(|v| *v += 1)
        .or_insert(1);
}

/// Reward a named counter.
pub fn reward(shared: &SharedState, name: &str) {
    shared
        .rewards
        .entry(name.to_string())
        .and_modify(|v| *v += 1)
        .or_insert(1);
}

// ── Server bootstrap ────────────────────────────────────────────────────────

/// Boot an organ server. Blocks forever. Generic over state type.
pub async fn run_organ(config: OrganConfig, app: Router<()>) {
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {} on {}: {}", config.name, addr, e));
    tracing::info!(
        "soullink-{} v{} listening on port {} (N={})",
        config.name,
        config.version,
        config.port,
        config.n
    );
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap_or_else(|e| panic!("{} server error: {}", config.name, e));
}
