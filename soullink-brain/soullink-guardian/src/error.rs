//! Error types for Core Guardian

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GuardianError {
    #[error("NVML initialization failed: {0}")]
    NvmlInit(String),

    #[error("GPU device not found: index {0}")]
    DeviceNotFound(u32),

    #[error("Thermal threshold exceeded: {temp_c}°C > {threshold}°C")]
    ThermalExceeded { temp_c: f32, threshold: f32 },

    #[error("XID error count exceeded: {count} >= {limit}")]
    XidThreshold { count: u64, limit: u64 },

    #[error("GPU hang detected: {duration_ms}ms unresponsive")]
    GpuHang { duration_ms: u64 },

    #[error("PCIe reset failed: {reason}")]
    ResetFailed { reason: String },

    #[error("WebSocket connection error: {0}")]
    WsError(String),

    #[error("Config error: {0}")]
    ConfigError(String),
}
