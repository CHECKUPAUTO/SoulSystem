//! ThermalGuard — 1 s NVML poll, emits `ThermalReading` events.
//!
//! V13.5-p1a hotfix: the `should_throttle` computation no longer ORs in
//! `throttle_reasons != 0`. That was semantically wrong — a loaded GPU
//! legitimately reports SW_POWER_CAP and HW_THERMAL_SLOWDOWN during normal
//! operation, and treating "clock is currently limited" as "we must enter
//! thermal alert" produces constant-on behaviour under any real workload.
//!
//! Throttle decision is now purely temperature-based, with the hysteresis
//! logic owned by DecisionEngine. Throttle reasons are surfaced via the
//! SystemSnapshot for observability (metrics + /state), not used here.

use std::{sync::Arc, time::Duration};

use tokio::{sync::{broadcast, mpsc}, time};
use tracing::{info, warn};
use uuid::Uuid;

use soullink_core::GuardianEvent;
use soullink_nvml::NvmlBridge;

pub struct ThermalGuard {
    nvml:          Arc<NvmlBridge>,
    tx:            mpsc::Sender<GuardianEvent>,
    shutdown:      broadcast::Receiver<()>,
    threshold_c:   f32,
    poll_interval: Duration,
    alert_on:      bool,
}

impl ThermalGuard {
    pub fn new(
        nvml: Arc<NvmlBridge>,
        tx: mpsc::Sender<GuardianEvent>,
        shutdown: broadcast::Receiver<()>,
        threshold_c: f32,
        poll_interval: Duration,
    ) -> Self {
        Self { nvml, tx, shutdown, threshold_c, poll_interval, alert_on: false }
    }

    pub async fn run(mut self) {
        info!(
            threshold_c = self.threshold_c,
            interval = ?self.poll_interval,
            "ThermalGuard started (V13.5-p1a: temperature-only signal)"
        );
        let mut ticker = time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => self.tick().await,
                _ = self.shutdown.recv() => { info!("ThermalGuard: shutdown"); break; }
            }
        }
    }

    async fn tick(&mut self) {
        let temp_c = self.nvml.gpu_temp();
        let power_w = self.nvml.power_w();

        // Temperature-only alert. Throttle reasons are observational and
        // flow through SystemSnapshot, not through this decision path.
        let alert = temp_c > self.threshold_c;

        if alert && !self.alert_on {
            warn!(temp_c, threshold_c = self.threshold_c, "THERMAL ALERT");
            self.alert_on = true;
        } else if !alert && self.alert_on {
            info!(temp_c, "thermal alert cleared");
            self.alert_on = false;
        }

        let ev = GuardianEvent::ThermalReading {
            temp_c,
            power_w,
            throttle: alert,
            correlation: Uuid::new_v4(),
        };
        if let Err(e) = self.tx.try_send(ev) {
            warn!(%e, "ThermalGuard: bus full, event dropped");
        }
    }
}
