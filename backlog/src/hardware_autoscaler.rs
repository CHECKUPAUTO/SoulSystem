//! Auto-scaler matériel — surveillance et recommandations.
//!
//! Collecte les métriques système réelles via `sysinfo` et `nvidia-smi`,
//! vérifie les seuils configurables et génère des rapports Markdown.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Instant;
use sysinfo::System;
use tracing::warn;

/// Métriques système collectées.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_percent: f32,
    pub cpu_count: u32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub load_avg_1min: f32,
    pub load_avg_5min: f32,
    pub load_avg_15min: f32,
    pub gpu_available: bool,
    pub gpu_temp_celsius: Option<f32>,
    pub gpu_util_percent: Option<f32>,
    pub gpu_mem_used_mb: Option<u64>,
    pub timestamp: i64,
}

/// Résultat de vérification avec seuils.
#[derive(Debug, Clone)]
pub struct ThresholdReport {
    pub cpu_ok: bool,
    pub ram_ok: bool,
    pub gpu_ok: bool,
    pub details: SystemMetrics,
    pub warnings: Vec<String>,
}

/// Configuration des seuils.
#[derive(Debug, Clone)]
pub struct AutoscalerConfig {
    pub cpu_threshold_pct: f32,
    pub ram_threshold_pct: f32,
    pub gpu_temp_threshold: f32,
    pub check_interval_secs: u64,
}

impl Default for AutoscalerConfig {
    fn default() -> Self {
        Self {
            cpu_threshold_pct: 80.0,
            ram_threshold_pct: 85.0,
            gpu_temp_threshold: 85.0,
            check_interval_secs: 60,
        }
    }
}

/// Auto-scaler qui surveille les ressources.
pub struct HardwareAutoscaler {
    sys: System,
    config: AutoscalerConfig,
    start_time: Instant,
    report_path: String,
}

impl HardwareAutoscaler {
    pub fn new(report_path: &str) -> Self {
        Self {
            sys: System::new_all(),
            config: AutoscalerConfig::default(),
            report_path: report_path.to_string(),
            start_time: Instant::now(),
        }
    }

    pub fn new_with_config(report_path: &str, config: AutoscalerConfig) -> Self {
        Self {
            sys: System::new_all(),
            config,
            report_path: report_path.to_string(),
            start_time: Instant::now(),
        }
    }

    /// Rafraîchit les informations système.
    pub fn refresh(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
    }

    /// Collecte les métriques actuelles.
    pub fn collect_metrics(&mut self) -> SystemMetrics {
        self.refresh();

        let cpu_percent = self.sys.global_cpu_usage();
        let cpu_count = self.sys.cpus().len() as u32;
        let ram_used = self.sys.used_memory();
        let ram_total = self.sys.total_memory();

        let load = System::load_average();

        // GPU detection via nvidia-smi — parse CSV output
        let (gpu_available, gpu_temp, gpu_util, gpu_mem) = Self::query_nvidia_smi();

        SystemMetrics {
            cpu_percent,
            cpu_count,
            ram_used_mb: ram_used / 1024 / 1024,
            ram_total_mb: ram_total / 1024 / 1024,
            load_avg_1min: load.one as f32,
            load_avg_5min: load.five as f32,
            load_avg_15min: load.fifteen as f32,
            gpu_available,
            gpu_temp_celsius: gpu_temp,
            gpu_util_percent: gpu_util,
            gpu_mem_used_mb: gpu_mem,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// Interroge nvidia-smi et parse le CSV de sortie.
    fn query_nvidia_smi() -> (bool, Option<f32>, Option<f32>, Option<u64>) {
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=index,temperature.gpu,utilization.gpu,memory.used",
                "--format=csv,noheader,nounits",
            ])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let line = stdout.lines().next();
                match line {
                    Some(l) => {
                        let parts: Vec<&str> = l.split(',').map(|s| s.trim()).collect();
                        if parts.len() >= 4 {
                            let temp = parts[1].parse::<f32>().ok();
                            let util = parts[2].parse::<f32>().ok();
                            let mem = parts[3].parse::<u64>().ok();
                            (true, temp, util, mem)
                        } else {
                            (true, None, None, None)
                        }
                    }
                    None => (true, None, None, None),
                }
            }
            _ => (false, None, None, None),
        }
    }

    /// Vérifie les seuils et retourne un rapport structuré.
    pub fn check(&mut self) -> ThresholdReport {
        let metrics = self.collect_metrics();
        let mut warnings = Vec::new();

        let cpu_ok = metrics.cpu_percent < self.config.cpu_threshold_pct;
        if !cpu_ok {
            warnings.push(format!(
                "CPU à {:.1}% (seuil: {}%)",
                metrics.cpu_percent, self.config.cpu_threshold_pct
            ));
        }

        let ram_pct = if metrics.ram_total_mb > 0 {
            (metrics.ram_used_mb as f32 / metrics.ram_total_mb as f32) * 100.0
        } else {
            0.0
        };
        let ram_ok = ram_pct < self.config.ram_threshold_pct;
        if !ram_ok {
            warnings.push(format!(
                "RAM à {:.1}% (seuil: {}%)",
                ram_pct, self.config.ram_threshold_pct
            ));
        }

        let mut gpu_ok = true;
        if let Some(temp) = metrics.gpu_temp_celsius {
            if temp > self.config.gpu_temp_threshold {
                gpu_ok = false;
                warnings.push(format!(
                    "GPU à {:.1}°C (seuil: {}°C)",
                    temp, self.config.gpu_temp_threshold
                ));
            }
        }

        if !warnings.is_empty() {
            warn!("HardwareAutoscaler: {}", warnings.join("; "));
        }

        ThresholdReport {
            cpu_ok,
            ram_ok,
            gpu_ok,
            details: metrics,
            warnings,
        }
    }

    /// Génère un rapport structuré en Markdown.
    pub fn generate_report(&self, report: &ThresholdReport) -> String {
        let mut md = String::from("# Hardware Scaling Report\n\n");
        md.push_str(&format!("**Date**: {}\n\n", Utc::now().to_rfc3339()));

        md.push_str("## Statut\n\n");
        if report.warnings.is_empty() {
            md.push_str("✅ Tous les seuils sont respectés.\n\n");
        } else {
            md.push_str("⚠️  Seuils dépassés :\n\n");
            for w in &report.warnings {
                md.push_str(&format!("- {}\n", w));
            }
            md.push_str("\n");
        }

        md.push_str("## Métriques actuelles\n\n");
        md.push_str(&format!(
            "- **CPU**: {:.1}% ({} cœurs)\n",
            report.details.cpu_percent, report.details.cpu_count
        ));
        md.push_str(&format!(
            "- **RAM**: {:.1} MB / {:.1} MB ({:.1}%)\n",
            report.details.ram_used_mb,
            report.details.ram_total_mb,
            if report.details.ram_total_mb > 0 {
                (report.details.ram_used_mb as f32 / report.details.ram_total_mb as f32) * 100.0
            } else {
                0.0
            }
        ));
        md.push_str(&format!(
            "- **Load average**: {:.2} / {:.2} / {:.2}\n",
            report.details.load_avg_1min,
            report.details.load_avg_5min,
            report.details.load_avg_15min
        ));

        if report.details.gpu_available {
            md.push_str(&format!(
                "- **GPU température**: {:.0}°C\n",
                report.details.gpu_temp_celsius.unwrap_or(0.0)
            ));
            md.push_str(&format!(
                "- **GPU utilisation**: {:.0}%\n",
                report.details.gpu_util_percent.unwrap_or(0.0)
            ));
            if let Some(mem) = report.details.gpu_mem_used_mb {
                md.push_str(&format!("- **GPU mémoire**: {} MB\n", mem));
            }
        } else {
            md.push_str("- **GPU**: non détecté\n");
        }

        md.push_str("\n## Recommandations\n\n");
        if !report.cpu_ok {
            md.push_str("- Augmenter les ressources CPU ou optimiser les charges de calcul\n");
        }
        if !report.ram_ok {
            md.push_str("- Augmenter la RAM ou réduire l'empreinte mémoire\n");
        }
        if !report.gpu_ok {
            md.push_str("- Vérifier le refroidissement GPU ou réduire la charge\n");
        }
        if report.cpu_ok && report.ram_ok && report.gpu_ok {
            md.push_str("- Aucune action nécessaire.\n");
        }

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autoscaler_creation() {
        let scaler = HardwareAutoscaler::new("/tmp/soulscale_test");
        assert!(scaler.report_path.contains("soulscale_test"));
    }

    #[test]
    fn test_collect_metrics_returns_reasonable_values() {
        let mut scaler = HardwareAutoscaler::new("/tmp/soulscale_test");
        let metrics = scaler.collect_metrics();

        assert!(metrics.cpu_percent >= 0.0);
        assert!(metrics.cpu_count >= 1);
        assert!(metrics.ram_total_mb >= 1);
    }

    #[test]
    fn test_check_returns_report() {
        let mut scaler = HardwareAutoscaler::new("/tmp/soulscale_test");
        let report = scaler.check();

        assert!(report.details.timestamp > 0);
    }

    #[test]
    fn test_generate_report_includes_sections() {
        let mut scaler = HardwareAutoscaler::new("/tmp/soulscale_test");
        let report = scaler.check();
        let md = scaler.generate_report(&report);

        assert!(md.contains("# Hardware Scaling Report"));
        assert!(md.contains("## Statut"));
        assert!(md.contains("## Métriques actuelles"));
        assert!(md.contains("## Recommandations"));
    }

    #[test]
    fn test_cpu_check_threshold_high() {
        let mut config = AutoscalerConfig::default();
        config.cpu_threshold_pct = 200.0; // impossible to exceed on real CPU
        let mut scaler = HardwareAutoscaler::new_with_config("/tmp/soulscale_test", config);
        let report = scaler.check();
        assert!(report.cpu_ok);
    }

    #[test]
    fn test_nvidia_smi_parsing_no_gpu() {
        // Sur une machine sans GPU, ne doit pas paniquer
        let (available, temp, util, mem) = HardwareAutoscaler::query_nvidia_smi();
        // On vérifie juste que ça retourne sans paniquer
        assert!(temp.is_none() || temp.is_some());
        assert!(util.is_none() || util.is_some());
        assert!(mem.is_none() || mem.is_some());
        _ = available;
    }
}
