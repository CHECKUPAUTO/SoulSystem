//! HomeoSync — 5 s batch, computes stress_index, pushes snapshot + stimulates Homeostasis organ.
//!
//! Phase 1 changes:
//! * **disk writes removed** — the JSON dump to
//!   `/mnt/nvme/.../homeosync_state.json` is gone, per Zero-Disk Policy. Any
//!   consumer (legacy decision-engine binary during cohabitation) reads the
//!   state over HTTP `GET /state` on the metrics port.
//! * the `reqwest::Client` is still per-tick here; phase 2 will hoist it to a
//!   shared `OnceLock<Client>` with a keep-alive pool.

use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use chrono::Utc;
use tokio::{sync::{broadcast, mpsc}, time};
use tracing::{debug, info};
use uuid::Uuid;

use soullink_core::{http::shared_client, GuardianEvent, SystemSnapshot};
use soullink_nvml::NvmlBridge;

pub struct HomeoSync {
    nvml:     Arc<NvmlBridge>,
    state:    Arc<ArcSwap<SystemSnapshot>>,
    tx:       mpsc::Sender<GuardianEvent>,
    shutdown: broadcast::Receiver<()>,
    interval: Duration,
}

impl HomeoSync {
    pub fn new(
        nvml: Arc<NvmlBridge>,
        state: Arc<ArcSwap<SystemSnapshot>>,
        tx: mpsc::Sender<GuardianEvent>,
        shutdown: broadcast::Receiver<()>,
        interval: Duration,
    ) -> Self {
        Self { nvml, state, tx, shutdown, interval }
    }

    pub async fn run(mut self) {
        info!(interval = ?self.interval, "HomeoSync started");
        let mut ticker = time::interval(self.interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => self.tick().await,
                _ = self.shutdown.recv() => { info!("HomeoSync: shutdown"); break; }
            }
        }
    }

    async fn tick(&self) {
        let temp_c   = self.nvml.gpu_temp();
        let util_pct = self.nvml.gpu_util();
        let mem_used = self.nvml.vram_used_mib();
        let mem_total = self.nvml.vram_total_mib();
        let power_w  = self.nvml.power_w();
        let fan      = self.nvml.fan_speed();
        let sm_clock = self.nvml.clock_sm_mhz();
        let mem_clock = self.nvml.clock_mem_mhz();
        let power_limit_w = self.nvml.power_limit_w();
        let throttle_reasons = self.nvml.throttle_reasons();

        let snap = SystemSnapshot {
            ts: Utc::now(),
            correlation_id: Uuid::new_v4(),
            gpu_temp_c: temp_c,
            gpu_util_pct: util_pct,
            gpu_mem_used_mib: mem_used,
            gpu_mem_total_mib: mem_total,
            gpu_power_w: power_w,
            fan_speed_pct: fan,
            sm_clock_mhz: sm_clock,
            mem_clock_mhz: mem_clock,
            throttle_reasons,
            power_limit_w,
            ..SystemSnapshot::default()
        };

        // Stress index (0..1)
        let temp_norm = (temp_c as f64 / 100.0).min(1.0);
        let vram_norm = if mem_total > 0 { mem_used as f64 / mem_total as f64 } else { 0.0 };
        let util_norm = util_pct as f64 / 100.0;
        let stress = 0.35 * temp_norm + 0.25 * vram_norm + 0.15 * util_norm + 0.25 * 0.5;

        self.state.store(Arc::new(snap.clone()));
        debug!(temp_c, util_pct, stress, power_limit_w, "snapshot updated");

        let action = if stress > 0.7 { "throttle" } else if stress > 0.5 { "observe" } else { "normal" };
        let _ = self.tx.try_send(GuardianEvent::StressUpdate { index: stress, action: action.into() });
        let _ = self.tx.try_send(GuardianEvent::MetricsBatch { snapshot: Arc::new(snap) });

        // Stimulate Homeostasis organ (9041) — kept for backward compat.
        // V13.5-p2b: uses the shared process-wide reqwest client to preserve
        // keep-alive across ticks (was being recreated every 5 s).
        let spike_count = (stress * 10.0) as usize * 5;
        if spike_count > 0 {
            let _ = shared_client()
                .post("http://127.0.0.1:9041/api/stimulate")
                .json(&serde_json::json!({ "count": spike_count }))
                .timeout(Duration::from_secs(2))
                .send()
                .await;
        }
    }
}
