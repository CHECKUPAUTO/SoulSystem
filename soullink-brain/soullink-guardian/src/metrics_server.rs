//! Prometheus `/metrics` + `/state` JSON endpoint.
//!
//! Phase 1 addition: `GET /state` returns the current `SystemSnapshot` as JSON.
//! Consumers that used to read `/mnt/nvme/.../homeosync_state.json` (legacy
//! decision-engine during cohabitation) now call this endpoint — implements
//! Zero-Disk Policy on the read path.

use std::{net::SocketAddr, sync::Arc};

use arc_swap::ArcSwap;
use axum::{extract::State, routing::get, Router};

use soullink_core::SystemSnapshot;

pub struct MetricsServer {
    port:  u16,
    state: Arc<ArcSwap<SystemSnapshot>>,
}

impl MetricsServer {
    pub fn new(port: u16, state: Arc<ArcSwap<SystemSnapshot>>) -> Self {
        Self { port, state }
    }

    pub async fn run(self) {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        tracing::info!(%addr, "metrics+state server listening");

        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/state", get(state_handler))
            .route("/health", get(|| async { "ok" }))
            .with_state(self.state.clone());

        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!(%e, "metrics server error");
                }
            }
            Err(e) => tracing::error!(%e, port = self.port, "failed to bind metrics port"),
        }
    }
}

async fn metrics_handler(State(state): State<Arc<ArcSwap<SystemSnapshot>>>) -> String {
    let s = state.load();
    format!(
        "# HELP gpu_temp GPU temperature (°C)\n# TYPE gpu_temp gauge\ngpu_temp {}\n\
         # HELP gpu_util GPU utilization (%)\n# TYPE gpu_util gauge\ngpu_util {}\n\
         # HELP gpu_vram_used VRAM used (MiB)\n# TYPE gpu_vram_used gauge\ngpu_vram_used {}\n\
         # HELP gpu_vram_total VRAM total (MiB)\n# TYPE gpu_vram_total gauge\ngpu_vram_total {}\n\
         # HELP gpu_power_w GPU power (W)\n# TYPE gpu_power_w gauge\ngpu_power_w {}\n\
         # HELP gpu_power_limit_w Enforced NVML power limit (W)\n# TYPE gpu_power_limit_w gauge\ngpu_power_limit_w {}\n\
         # HELP gpu_fan_speed Fan (%)\n# TYPE gpu_fan_speed gauge\ngpu_fan_speed {}\n\
         # HELP gpu_sm_clock SM clock (MHz)\n# TYPE gpu_sm_clock gauge\ngpu_sm_clock {}\n\
         # HELP gpu_mem_clock Mem clock (MHz)\n# TYPE gpu_mem_clock gauge\ngpu_mem_clock {}\n\
         # HELP gpu_throttle_reasons NVML throttle reason bitmask\n# TYPE gpu_throttle_reasons gauge\ngpu_throttle_reasons {}\n\
         # HELP xid_errors XID error count\n# TYPE xid_errors counter\nxid_errors {}\n\
         # HELP throttle_active 1 if throttle engaged\n# TYPE throttle_active gauge\nthrottle_active {}\n",
        s.gpu_temp_c, s.gpu_util_pct, s.gpu_mem_used_mib, s.gpu_mem_total_mib,
        s.gpu_power_w, s.power_limit_w,
        s.fan_speed_pct, s.sm_clock_mhz, s.mem_clock_mhz, s.throttle_reasons,
        s.xid_error_count, s.throttle_active as u8
    )
}

async fn state_handler(State(state): State<Arc<ArcSwap<SystemSnapshot>>>) -> axum::Json<SystemSnapshot> {
    axum::Json(state.load().as_ref().clone())
}
