//! Auto-scaler matériel — surveillance et recommandations.
//!
//! Collecte les métriques système et génère des rapports
//! si les ressources sont saturées. N'effectue jamais d'action automatique.

use chrono::Utc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Métriques système collectées.
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub cpu_percent: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub gpu_available: bool,
    pub gpu_temp_celsius: Option<f32>,
    pub hnn_ticks_per_sec: u64,
    pub timestamp: i64,
}

/// Auto-scaler qui surveille les ressources.
pub struct HardwareAutoscaler {
    start_time: Instant,
    high_cpu_since: Option<Instant>,
    report_path: String,
}

impl HardwareAutoscaler {
    pub fn new(report_path: &str) -> Self {
        Self {
            start_time: Instant::now(),
            high_cpu_since: None,
            report_path: report_path.to_string(),
        }
    }

    /// Collecte les métriques actuelles.
    pub fn collect_metrics(&self) -> SystemMetrics {
        // En production: utilise sysinfo crate
        SystemMetrics {
            cpu_percent: 0.0,    // sysinfo::System::new_all().global_cpu_usage()
            ram_used_mb: 0,      // sysinfo
            ram_total_mb: 16000, // placeholder
            gpu_available: false,
            gpu_temp_celsius: None,
            hnn_ticks_per_sec: 254_000,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// Vérifie les seuils et génère un rapport si nécessaire.
    pub fn check_thresholds(&mut self, metrics: &SystemMetrics) -> Option<String> {
        if metrics.cpu_percent > 90.0 {
            if self.high_cpu_since.is_none() {
                self.high_cpu_since = Some(Instant::now());
                warn!("HardwareAutoscaler: CPU > 90% — monitoring...");
            } else if self.high_cpu_since.unwrap().elapsed() > Duration::from_secs(300) {
                // 5 minutes de CPU élevé → rapport
                let report = self.generate_report(metrics);
                info!("HardwareAutoscaler: report generated");
                return Some(report);
            }
        } else {
            self.high_cpu_since = None;
        }
        None
    }

    fn generate_report(&self, metrics: &SystemMetrics) -> String {
        let mut report = String::from("# Hardware Scaling Report\n\n");
        report.push_str(&format!("**Date**: {}\n\n", Utc::now().to_rfc3339()));
        report.push_str("## Current Metrics\n\n");
        report.push_str(&format!("- CPU: {:.1}%\n", metrics.cpu_percent));
        report.push_str(&format!(
            "- RAM: {} MB / {} MB\n",
            metrics.ram_used_mb, metrics.ram_total_mb
        ));
        report.push_str(&format!("- HNN: {} ticks/sec\n", metrics.hnn_ticks_per_sec));
        report.push_str("\n## Recommendations\n\n");
        report.push_str("- Consider upgrading CPU or adding more cores\n");
        report.push_str("- Evaluate GPU offloading for HNN\n");
        report
    }
}
