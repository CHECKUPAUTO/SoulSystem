//! Unix Domain Socket client — Orchestrator side.
//!
//! Connects to the Guardian's UDS (`/run/soullink/guardian.sock`), reads
//! length-prefixed MessagePack frames indefinitely, and forwards each
//! decoded `SystemSnapshot` to a callback. Reconnects with exponential
//! backoff on disconnect.
//!
//! The HTTP endpoint `POST /api/guardian/snapshot` is kept as the
//! fallback path; when UDS is available it takes precedence, the HTTP
//! handler is merely a no-op for snapshots that also arrived via UDS.

use std::{path::PathBuf, sync::Arc, time::Duration};

use serde_json::{json, Value};
use tokio::net::UnixStream;
use tracing::{debug, info, warn};

use soullink_core::{ipc::read_frame, SystemSnapshot};

pub struct IpcClient {
    socket_path: PathBuf,
    sink: Arc<dyn Fn(SystemSnapshot) + Send + Sync>,
}

impl IpcClient {
    pub fn new<F>(socket_path: impl Into<PathBuf>, sink: F) -> Self
    where
        F: Fn(SystemSnapshot) + Send + Sync + 'static,
    {
        Self {
            socket_path: socket_path.into(),
            sink: Arc::new(sink),
        }
    }

    /// Runs forever — connect, read frames, reconnect on error.
    /// Intended to be spawned with `tokio::spawn` and left running for the
    /// lifetime of the process. Cancellation is via dropping the task
    /// handle (orchestrator terminates on Ctrl+C / SIGTERM, which tears
    /// down the runtime and all its tasks).
    pub async fn run(self) {
        info!(path = %self.socket_path.display(), "IpcClient starting (UDS)");
        let mut backoff_ms: u64 = 500;

        loop {
            match UnixStream::connect(&self.socket_path).await {
                Ok(mut stream) => {
                    info!(path = %self.socket_path.display(), "IpcClient connected");
                    backoff_ms = 500;

                    loop {
                        match read_frame::<SystemSnapshot, _>(&mut stream).await {
                            Ok(snap) => {
                                debug!(
                                    temp_c = snap.gpu_temp_c,
                                    throttle = snap.throttle_active,
                                    "IpcClient: snapshot received"
                                );
                                (self.sink)(snap);
                            }
                            Err(e) => {
                                warn!(%e, "IpcClient: read failed; reconnecting");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!(%e, backoff_ms,
                           "IpcClient: connect failed (guardian not up?); backing off");
                }
            }

            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(30_000);
        }
    }
}

/// Helper: build a sink closure that stores the snapshot in the
/// orchestrator's `guardian` DashMap under `"latest"`.
pub fn dashmap_sink(
    guardian: Arc<dashmap::DashMap<String, Value>>,
) -> impl Fn(SystemSnapshot) + Send + Sync + 'static {
    move |snap: SystemSnapshot| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        guardian.insert(
            "latest".to_string(),
            json!({
                "ts": snap.ts.to_rfc3339(),
                "correlation_id": snap.correlation_id.to_string(),
                "gpu_temp_c":        snap.gpu_temp_c,
                "gpu_util_pct":      snap.gpu_util_pct,
                "gpu_mem_used_mib":  snap.gpu_mem_used_mib,
                "gpu_mem_total_mib": snap.gpu_mem_total_mib,
                "gpu_power_w":       snap.gpu_power_w,
                "power_limit_w":     snap.power_limit_w,
                "fan_speed_pct":     snap.fan_speed_pct,
                "sm_clock_mhz":      snap.sm_clock_mhz,
                "mem_clock_mhz":     snap.mem_clock_mhz,
                "throttle_active":   snap.throttle_active,
                "throttle_reasons":  snap.throttle_reasons,
                "xid_error_count":   snap.xid_error_count,
                "received_at":       ts,
                "via":               "uds",
            }),
        );
    }
}
