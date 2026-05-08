//! Soul Decision Engine V13 — Rust native
//!
//! Phase 1 changes:
//! * **Zero-Disk Policy (read side)** — stress is read from the Guardian's
//!   `GET /state` HTTP endpoint (port 9091) instead of polling
//!   `/mnt/nvme/.../homeosync_state.json`.
//! * **Zero-Disk Policy (write side)** — `synthetic_trades.log` is replaced
//!   by `tracing::info!` with a dedicated `trade!` event, captured by
//!   journald. Consumers that relied on the log file should switch to
//!   `journalctl -u soul-decision-v13 --grep=TRADE`.
//! * Shared `reqwest::Client` with keep-alive, instead of one per tick.
//! * Auth: propagates `SOULLINK_INTERNAL_TOKEN` to Guardian calls.
//!
//! This binary will be folded into the Guardian actor tree in phase 3 and
//! this crate removed.

use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::time::interval;

const GUARDIAN_STATE_URL: &str = "http://127.0.0.1:9091/state";
const SCIENCE_URL:        &str = "http://127.0.0.1:9010/api/stats";

#[derive(Debug, Clone, Deserialize, Default)]
struct BrainStats {
    #[serde(default)]
    #[allow(dead_code)]
    spike_rate: f64,
    #[serde(default)]
    turbulence: TurbulenceInfo,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TurbulenceInfo {
    #[serde(default)]
    value: f64,
    #[serde(default, rename = "attractor")]
    attractor: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GuardianState {
    #[serde(default)]
    gpu_temp_c: f32,
    #[serde(default)]
    gpu_util_pct: u32,
    #[serde(default)]
    gpu_mem_used_mib: u64,
    #[serde(default)]
    gpu_mem_total_mib: u64,
    #[serde(default)]
    throttle_active: bool,
}

fn compute_stress(g: &GuardianState) -> f64 {
    let temp_norm = (g.gpu_temp_c as f64 / 100.0).min(1.0);
    let vram_norm = if g.gpu_mem_total_mib > 0 {
        g.gpu_mem_used_mib as f64 / g.gpu_mem_total_mib as f64
    } else { 0.0 };
    let util_norm = g.gpu_util_pct as f64 / 100.0;
    0.35 * temp_norm + 0.25 * vram_norm + 0.15 * util_norm + 0.25 * 0.5
}

/// Cosine similarity between two vectors (AVX2-optimized when compiled with target-cpu=broadwell).
/// Used by the decision engine to compare current state against historical decisions.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    dot / (norm_a * norm_b)
}

#[tokio::main(worker_threads = 2)]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("⚡ Soul Decision Engine V13.5 — Rust Native | 10Hz | /state over HTTP (Zero-Disk)");

    let token = std::env::var("SOULLINK_INTERNAL_TOKEN").ok();

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(8)
        .pool_idle_timeout(Duration::from_secs(60))
        .tcp_keepalive(Duration::from_secs(30))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("reqwest");

    let mut last_log   = Instant::now() - Duration::from_secs(60);
    let mut last_trade = Instant::now() - Duration::from_secs(60);
    let mut tick       = interval(Duration::from_millis(100));

    loop {
        tick.tick().await;

        // 1. Fetch brain stats
        let mut brain_req = client.get(SCIENCE_URL);
        if let Some(t) = &token { brain_req = brain_req.bearer_auth(t); }
        let brain: BrainStats = match brain_req.send().await {
            Ok(r) => r.json().await.unwrap_or_default(),
            Err(_) => continue,
        };

        // 2. Fetch Guardian state (replaces JSON file read)
        let mut guardian_req = client.get(GUARDIAN_STATE_URL);
        if let Some(t) = &token { guardian_req = guardian_req.bearer_auth(t); }
        let g: GuardianState = match guardian_req.send().await {
            Ok(r) => r.json().await.unwrap_or_default(),
            Err(_) => GuardianState::default(),
        };

        let stress = compute_stress(&g);
        let voltage = -70.0 - (brain.turbulence.value * 5.0);
        let g_drive = 100.0 * (1.0 - brain.turbulence.value) + stress * 10.0;

        // V13.5-p1a: when Guardian reports throttle active, force HOLD
        // regardless of attractor. Don't execute trades on a thermally
        // limited GPU — latency is unpredictable and stress is amplified.
        let (action, conf) = if g.throttle_active {
            ("HOLD_THROTTLE", 0.95)
        } else { match brain.turbulence.attractor.as_str() {
            "StrangeAttractor" if voltage < -74.5 => ("EXECUTE_SHORT", 0.92),
            "StrangeAttractor" if voltage > -72.5 => ("EXECUTE_LONG", 0.88),
            "StrangeAttractor"                    => ("HOLD_OBSERVE", 0.60),
            "Transient"                           => ("HOLD_TRANSITION", 0.45),
            "StableOrbit"                         => ("CONSOLIDATE", 0.70),
            "DeepBasin"                           => ("IDLE", 0.80),
            _                                     => ("UNKNOWN", 0.30),
        }};

        let now = Instant::now();

        if (action.starts_with("EXECUTE") || action == "CONSOLIDATE")
            && now.duration_since(last_log) > Duration::from_secs(30)
        {
            tracing::info!(
                action, voltage, drive = g_drive, stress, confidence = conf,
                attractor = %brain.turbulence.attractor,
                "⚡ decision"
            );
            last_log = now;
        }

        if action.starts_with("EXECUTE") && now.duration_since(last_trade) > Duration::from_secs(60) {
            // No file write — emitted as structured log for journald.
            tracing::info!(
                target: "trade",
                action, voltage, drive = g_drive, stress,
                hz = brain.spike_rate,
                "TRADE"
            );
            last_trade = now;
        }
    }
}
