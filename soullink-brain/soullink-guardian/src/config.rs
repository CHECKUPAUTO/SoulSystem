//! Configuration loader — TOML `/etc/soullink/guardian.toml` + env var overrides
//! (`CG_*` prefix).
//!
//! Phase 1 changes:
//! * **`throttle_command` field removed** — it was a free-form shell string
//!   and the top offender for injection. Power-limit is now a numeric
//!   [`throttle_power_limit_w`] applied via NVML FFI.
//! * [`reset_power_limit_w`] — the watts to restore when throttle is lifted.
//! * All TOML fields are now actually loaded (legacy loader ignored 7 of them).

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianConfig {
    #[serde(default = "default_orchestrator_url")]
    pub orchestrator_url: String,

    #[serde(default)]
    pub gpu_device_index: u32,

    #[serde(default = "default_thermal_threshold_c")]
    pub thermal_threshold_c: f32,

    #[serde(with = "humantime_ms", default = "default_thermal_interval")]
    pub thermal_interval: Duration,

    #[serde(with = "humantime_ms", default = "default_homeo_interval")]
    pub homeo_interval: Duration,

    #[serde(default = "default_pcie_hang_threshold_ms")]
    pub pcie_hang_threshold_ms: u64,

    /// Phase 6b audit triage: kill-switch for the SM-clock stall
    /// heuristic. Default true; flip to false if you see repeated false
    /// positives (GPU pinned at boost clock is NOT a hang). The XID
    /// path (/dev/kmsg) remains authoritative regardless.
    #[serde(default = "default_hang_detector_enabled")]
    pub hang_detector_enabled: bool,

    #[serde(default = "default_reset_script")]
    pub reset_script: PathBuf,

    /// Wattage applied when throttle engages (replaces `nvidia-smi -pl` string).
    #[serde(default = "default_throttle_power_limit_w")]
    pub throttle_power_limit_w: u32,

    /// Wattage restored when throttle releases (replaces `nvidia-smi -pl 115`).
    #[serde(default = "default_reset_power_limit_w")]
    pub reset_power_limit_w: u32,

    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,

    #[serde(default = "default_emergency_stress_threshold")]
    pub emergency_stress_threshold: f64,

    #[serde(default = "default_emergency_turbulence_threshold")]
    pub emergency_turbulence_threshold: f64,

    #[serde(with = "humantime_ms", default = "default_xid_scan_interval")]
    pub xid_scan_interval: Duration,

    /// Phase 5: opt-in PI controller for continuous energy pacing. When
    /// disabled (default), the Phase 1 binary hysteresis runs as before.
    #[serde(default)]
    pub pacing_enabled: bool,

    #[serde(default = "default_pacing_target_c")]
    pub pacing_target_c: f32,

    #[serde(default = "default_pacing_hard_cap_c")]
    pub pacing_hard_cap_c: f32,
}

fn default_orchestrator_url() -> String { "http://127.0.0.1:9020".into() }
fn default_thermal_threshold_c() -> f32 { 85.0 }
fn default_thermal_interval() -> Duration { Duration::from_millis(1000) }
fn default_homeo_interval() -> Duration { Duration::from_millis(5000) }
fn default_pcie_hang_threshold_ms() -> u64 { 30_000 }
fn default_hang_detector_enabled() -> bool { true }
fn default_reset_script() -> PathBuf { PathBuf::from("/opt/soullink/scripts/pcie_reset.sh") }
fn default_throttle_power_limit_w() -> u32 { 90 }
fn default_reset_power_limit_w() -> u32 { 115 }
fn default_metrics_port() -> u16 { 9091 }
fn default_emergency_stress_threshold() -> f64 { 0.7 }
fn default_emergency_turbulence_threshold() -> f64 { 0.8 }
fn default_xid_scan_interval() -> Duration { Duration::from_millis(100) }
fn default_pacing_target_c() -> f32 { 75.0 }
fn default_pacing_hard_cap_c() -> f32 { 87.0 }

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            orchestrator_url: default_orchestrator_url(),
            gpu_device_index: 0,
            thermal_threshold_c: default_thermal_threshold_c(),
            thermal_interval: default_thermal_interval(),
            homeo_interval: default_homeo_interval(),
            pcie_hang_threshold_ms: default_pcie_hang_threshold_ms(),
            hang_detector_enabled: default_hang_detector_enabled(),
            reset_script: default_reset_script(),
            throttle_power_limit_w: default_throttle_power_limit_w(),
            reset_power_limit_w: default_reset_power_limit_w(),
            metrics_port: default_metrics_port(),
            emergency_stress_threshold: default_emergency_stress_threshold(),
            emergency_turbulence_threshold: default_emergency_turbulence_threshold(),
            xid_scan_interval: default_xid_scan_interval(),
            pacing_enabled: false,
            pacing_target_c: default_pacing_target_c(),
            pacing_hard_cap_c: default_pacing_hard_cap_c(),
        }
    }
}

impl GuardianConfig {
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var("CG_CONFIG_PATH")
            .unwrap_or_else(|_| "/etc/soullink/guardian.toml".to_string());

        // 1. Start with defaults, then layer TOML, then env overrides.
        let mut cfg: Self = if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(&path)?;
            toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("guardian.toml parse error at {path}: {e}"))?
        } else {
            tracing::warn!(path = %path, "guardian.toml not found — using defaults");
            Self::default()
        };

        // 2. Env overrides (CG_*)
        if let Ok(v) = std::env::var("CG_ORCHESTRATOR_URL")        { cfg.orchestrator_url = v; }
        if let Ok(v) = std::env::var("CG_GPU_DEVICE_INDEX")        { if let Ok(x) = v.parse() { cfg.gpu_device_index = x; } }
        if let Ok(v) = std::env::var("CG_THERMAL_THRESHOLD_C")     { if let Ok(x) = v.parse() { cfg.thermal_threshold_c = x; } }
        if let Ok(v) = std::env::var("CG_METRICS_PORT")            { if let Ok(x) = v.parse() { cfg.metrics_port = x; } }
        if let Ok(v) = std::env::var("CG_PCIE_HANG_THRESHOLD_MS")  { if let Ok(x) = v.parse() { cfg.pcie_hang_threshold_ms = x; } }
        if let Ok(v) = std::env::var("CG_THROTTLE_POWER_LIMIT_W")  { if let Ok(x) = v.parse() { cfg.throttle_power_limit_w = x; } }
        if let Ok(v) = std::env::var("CG_RESET_POWER_LIMIT_W")     { if let Ok(x) = v.parse() { cfg.reset_power_limit_w = x; } }
        if let Ok(v) = std::env::var("CG_RESET_SCRIPT")            { cfg.reset_script = PathBuf::from(v); }

        tracing::info!(
            ?cfg,
            "guardian config loaded"
        );
        Ok(cfg)
    }
}

// Serde adapter: TOML integer (ms) ↔ Duration
mod humantime_ms {
    use std::time::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_millis() as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = u64::deserialize(d)?;
        Ok(Duration::from_millis(ms))
    }
}
