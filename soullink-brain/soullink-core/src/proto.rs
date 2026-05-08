//! Wire protocol — types shared between guardian, orchestrator, gateway.
//!
//! These are the canonical shapes serialised on the Guardian→Orchestrator IPC
//! (currently HTTP/JSON, will migrate to Unix Domain Socket + MessagePack in
//! phase 3). Schema stability across v13.x is maintained via `#[serde(default)]`
//! on every field.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Full system snapshot emitted every 5 s by HomeoSync.
///
/// # Wire compatibility
/// All fields must remain backward-compatible; add new fields with
/// `#[serde(default)]` and never remove old ones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub ts:                 DateTime<Utc>,
    pub correlation_id:     Uuid,
    pub gpu_temp_c:         f32,
    pub gpu_util_pct:       u32,
    pub gpu_mem_used_mib:   u64,
    pub gpu_mem_total_mib:  u64,
    pub gpu_power_w:        f32,
    pub fan_speed_pct:      u32,
    #[serde(default)]
    pub pcie_rx_mbs:        f64,
    #[serde(default)]
    pub pcie_tx_mbs:        f64,
    #[serde(default)]
    pub xid_error_count:    u64,
    pub throttle_active:    bool,
    pub sm_clock_mhz:       u32,
    pub mem_clock_mhz:      u32,
    /// Throttle reasons bitmask (populated in phase 2 via NVML).
    #[serde(default)]
    pub throttle_reasons:   u64,
    /// Power-management limit currently applied, in watts.
    #[serde(default)]
    pub power_limit_w:      f32,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            ts:                Utc::now(),
            correlation_id:    Uuid::new_v4(),
            gpu_temp_c:        0.0,
            gpu_util_pct:      0,
            gpu_mem_used_mib:  0,
            gpu_mem_total_mib: 8188, // RTX 4060 8 GB
            gpu_power_w:       0.0,
            fan_speed_pct:     0,
            pcie_rx_mbs:       0.0,
            pcie_tx_mbs:       0.0,
            xid_error_count:   0,
            throttle_active:   false,
            sm_clock_mhz:      0,
            mem_clock_mhz:     0,
            throttle_reasons:  0,
            power_limit_w:     115.0,
        }
    }
}

/// Events flowing on the internal MPSC bus (actors → DecisionEngine).
#[derive(Debug, Clone)]
pub enum GuardianEvent {
    ThermalReading {
        temp_c:      f32,
        power_w:     f32,
        throttle:    bool,
        correlation: Uuid,
    },
    MetricsBatch {
        snapshot: Arc<SystemSnapshot>,
    },
    XidError {
        code:  u32,
        count: u64,
    },
    GpuHang {
        duration_ms: u64,
    },
    ResetCompleted {
        success:    bool,
        elapsed_ms: u64,
    },
    StressUpdate {
        index:  f64,
        action: String,
    },
}
