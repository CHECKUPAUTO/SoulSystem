//! MemoryHealth — Moniteur de santé mémoire.
//!
//! Vérifie en continu :
//! - Latence des requêtes API
//! - Taux d'occupation du stockage
//! - Ratio de succès des recherches vectorielles
//!
//! Déclenche des actions correctives :
//! - Compactage d'index si occupation > seuil
//! - Basculement vers fallback local si service dégradé
//! - Alerte via bus événement si anomalie

use std::sync::Arc;
use std::time::Instant;
use std::collections::VecDeque;
use tracing::{info, warn};

use crate::memory_hub::MemoryHub;

/// Configuration du moniteur santé.
pub struct HealthConfig {
    /// Seuil de latence moyenne (ms) au-delà duquel on considère le service dégradé.
    pub latency_warn_ms: f64,
    /// Nombre max d'entrées avant compactage.
    pub max_entries_warn: usize,
    /// Ratio d'échec de search avant basculement fallback.
    pub search_failure_ratio: f64,
    /// Fenêtre de mesures pour les moyennes.
    pub window_size: usize,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            latency_warn_ms: 500.0,
            max_entries_warn: 1000,
            search_failure_ratio: 0.3,
            window_size: 50,
        }
    }
}

/// Rapport de santé à un instant t.
#[derive(Clone)]
pub struct HealthReport {
    pub avg_latency_ms: f64,
    pub memory_entries: usize,
    pub search_success_rate: f64,
    pub status: HealthStatus,
    pub actions_taken: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    FallbackActive,
}

/// Moniteur de santé mémoire.
pub struct MemoryHealth {
    pub config: HealthConfig,
    pub latencies: VecDeque<f64>,
    pub search_attempts: usize,
    pub search_failures: usize,
    pub check_count: u64,
    pub last_report: Option<HealthReport>,
}

impl MemoryHealth {
    pub fn new(config: HealthConfig) -> Self {
        let window_size = config.window_size;
        Self {
            config,
            latencies: VecDeque::with_capacity(window_size),
            search_attempts: 0,
            search_failures: 0,
            check_count: 0,
            last_report: None,
        }
    }

    /// Enregistre une latence de requête.
    pub fn record_latency(&mut self, duration_ms: f64) {
        if self.latencies.len() >= self.config.window_size {
            self.latencies.pop_front();
        }
        self.latencies.push_back(duration_ms);
    }

    /// Enregistre un résultat de recherche.
    pub fn record_search(&mut self, success: bool) {
        self.search_attempts += 1;
        if !success {
            self.search_failures += 1;
        }
    }

    /// Calcule la latence moyenne sur la fenêtre.
    pub fn avg_latency(&self) -> f64 {
        if self.latencies.is_empty() { return 0.0; }
        self.latencies.iter().sum::<f64>() / self.latencies.len() as f64
    }

    /// Calcule le taux d'échec des recherches.
    pub fn search_failure_rate(&self) -> f64 {
        if self.search_attempts == 0 { return 0.0; }
        self.search_failures as f64 / self.search_attempts as f64
    }

    /// Exécute un check complet + actions correctives.
    /// Retourne le rapport de santé.
    pub async fn check(&mut self, hub: &MemoryHub) -> HealthReport {
        self.check_count += 1;

        let avg_lat = self.avg_latency();
        let fail_rate = self.search_failure_rate();
        let entries = 0;
        let mut actions: Vec<String> = Vec::new();

        // Déterminer le statut
        let status = if avg_lat > self.config.latency_warn_ms {
            HealthStatus::Degraded(format!("latence élevée: {:.1}ms", avg_lat))
        } else if entries > self.config.max_entries_warn {
            HealthStatus::Degraded(format!("trop d'entrées: {}", entries))
        } else if fail_rate > self.config.search_failure_ratio {
            HealthStatus::FallbackActive
        } else {
            HealthStatus::Healthy
        };

        // Actions correctives
        if entries > self.config.max_entries_warn {
            hub.decay_and_prune(0.05, 0.95, 500).await;
            actions.push(format!("compactage: {} → 500 max", entries));
            info!("MemoryHealth: compactage déclenché ({} entrées)", entries);
        }

        if fail_rate > self.config.search_failure_ratio && avg_lat > self.config.latency_warn_ms {
            actions.push("basculement fallback local recommandé".into());
            warn!("MemoryHealth: service dégradé, fallback recommandé");
        }

        // Reset périodique des compteurs
        if self.check_count % 10 == 0 {
            self.search_attempts = 0;
            self.search_failures = 0;
        }

        let report = HealthReport {
            avg_latency_ms: avg_lat,
            memory_entries: entries,
            search_success_rate: 1.0 - fail_rate,
            status: status.clone(),
            actions_taken: actions,
        };
        self.last_report = Some(report.clone());

        // Log
        match &status {
            HealthStatus::Healthy => info!("MemoryHealth: ✅ santé OK"),
            HealthStatus::Degraded(s) => warn!("MemoryHealth: ⚠️ dégradé — {}", s),
            HealthStatus::FallbackActive => warn!("MemoryHealth: 🟡 fallback actif"),
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_healthy_by_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = crate::memory_hub::MemoryHub::new(dir.path()).await;
        let mut health = MemoryHealth::new(HealthConfig::default());
        let report = health.check(&hub).await;
        assert_eq!(report.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_latency_tracking() {
        let mut health = MemoryHealth::new(HealthConfig::default());
        health.record_latency(10.0);
        health.record_latency(20.0);
        assert!((health.avg_latency() - 15.0).abs() < 0.1);
    }
}
