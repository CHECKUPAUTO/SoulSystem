//! MemoryHealth — Moniteur de santé mémoire avec métriques de fatigue.
//!
//! Vérifie en continu :
//! - Latence des requêtes API
//! - Taux d'occupation du stockage
//! - Ratio de succès des recherches vectorielles
//! - Nombre de compactions dans l'heure
//! - Taux de messages par minute
//! - Taille estimée du contexte
//! - Variance d'α_sync (fatigue BCI)
//! - Taux d'erreur
//!
//! Déclenche des actions correctives :
//! - Compactage d'index si occupation > seuil
//! - Basculement vers fallback local si service dégradé
//! - Alerte de fatigue cognitive si métriques critiques
//! - Alerte via bus événement si anomalie

use std::collections::VecDeque;
use std::time::Instant;
use tracing::{info, warn};

use crate::memory_hub::MemoryHub;

// ── Configuration ──────────────────────────────────────────────────────

/// Configuration du moniteur santé.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Seuil de latence moyenne (ms) au-delà duquel on considère le service dégradé.
    pub latency_warn_ms: f64,
    /// Nombre max d'entrées avant compactage.
    pub max_entries_warn: usize,
    /// Ratio d'échec de search avant basculement fallback.
    pub search_failure_ratio: f64,
    /// Fenêtre de mesures pour les moyennes.
    pub window_size: usize,

    // ── Nouvelles métriques (S4) ───────────────────────────────────────
    /// Nombre max de compactions par heure avant alerte.
    pub max_compactions_per_hour: u64,
    /// Taux de messages par minute avant alerte de fatigue.
    pub max_message_rate: f64,
    /// Taille de contexte estimée (tokens) avant alerte.
    pub context_size_warn_tokens: usize,
    /// Variance d'α_sync maximale avant alerte de fatigue cognitive.
    pub max_alpha_sync_variance: f64,
    /// Taux d'erreur max avant alerte.
    pub max_error_rate: f64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            latency_warn_ms: 500.0,
            max_entries_warn: 1000,
            search_failure_ratio: 0.3,
            window_size: 50,
            max_compactions_per_hour: 2,
            max_message_rate: 30.0,
            context_size_warn_tokens: 500_000,
            max_alpha_sync_variance: 0.4,
            max_error_rate: 0.15,
        }
    }
}

// ── Types ──────────────────────────────────────────────────────────────

/// Rapport de santé à un instant t.
#[derive(Clone, Debug)]
pub struct HealthReport {
    pub avg_latency_ms: f64,
    pub memory_entries: usize,
    pub search_success_rate: f64,
    pub status: HealthStatus,
    pub actions_taken: Vec<String>,

    // Nouvelles métriques
    pub compactions_last_hour: u64,
    pub message_rate: f64,
    pub estimated_context_tokens: usize,
    pub alpha_sync_variance: f64,
    pub error_rate: f64,
    pub is_fatigued: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    FallbackActive,
    Fatigued(String),
}

// ── Moniteur ───────────────────────────────────────────────────────────

/// Moniteur de santé mémoire avec détection de fatigue.
pub struct MemoryHealth {
    pub config: HealthConfig,
    pub latencies: VecDeque<f64>,
    pub search_attempts: usize,
    pub search_failures: usize,
    pub check_count: u64,
    pub last_report: Option<HealthReport>,

    // Nouvelles métriques
    compaction_timestamps: VecDeque<Instant>,
    message_timestamps: VecDeque<Instant>,
    alpha_sync_samples: VecDeque<f64>,
    error_count: u64,
    total_operations: u64,
    current_context_estimate: usize,
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
            compaction_timestamps: VecDeque::new(),
            message_timestamps: VecDeque::new(),
            alpha_sync_samples: VecDeque::with_capacity(100),
            error_count: 0,
            total_operations: 0,
            current_context_estimate: 0,
        }
    }

    // ── Enregistrement des métriques ────────────────────────────────────

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

    /// Enregistre une compaction.
    pub fn record_compaction(&mut self) {
        self.compaction_timestamps.push_back(Instant::now());
        // Nettoyer les entrées de plus d'une heure
        let cutoff = Instant::now();
        while let Some(&ts) = self.compaction_timestamps.front() {
            if cutoff.duration_since(ts).as_secs() > 3600 {
                self.compaction_timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Enregistre un message (pour le taux de messages).
    pub fn record_message(&mut self) {
        self.message_timestamps.push_back(Instant::now());
        // Nettoyer les entrées de plus d'une minute
        let cutoff = Instant::now();
        while let Some(&ts) = self.message_timestamps.front() {
            if cutoff.duration_since(ts).as_secs() > 60 {
                self.message_timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Enregistre un échantillon α_sync (fatigue BCI).
    pub fn record_alpha_sync(&mut self, value: f64) {
        if self.alpha_sync_samples.len() >= 100 {
            self.alpha_sync_samples.pop_front();
        }
        self.alpha_sync_samples.push_back(value);
    }

    /// Enregistre une opération (succès ou échec).
    pub fn record_operation(&mut self, is_error: bool) {
        self.total_operations += 1;
        if is_error {
            self.error_count += 1;
        }
    }

    /// Définit la taille estimée du contexte.
    pub fn set_context_estimate(&mut self, tokens: usize) {
        self.current_context_estimate = tokens;
    }

    // ── Calculs ─────────────────────────────────────────────────────────

    /// Calcule la latence moyenne sur la fenêtre.
    pub fn avg_latency(&self) -> f64 {
        if self.latencies.is_empty() {
            return 0.0;
        }
        self.latencies.iter().sum::<f64>() / self.latencies.len() as f64
    }

    /// Calcule le taux d'échec des recherches.
    pub fn search_failure_rate(&self) -> f64 {
        if self.search_attempts == 0 {
            return 0.0;
        }
        self.search_failures as f64 / self.search_attempts as f64
    }

    /// Calcule le nombre de compactions dans l'heure.
    pub fn compactions_last_hour(&self) -> u64 {
        self.compaction_timestamps.len() as u64
    }

    /// Calcule le taux de messages par minute.
    pub fn message_rate(&self) -> f64 {
        self.message_timestamps.len() as f64
    }

    /// Calcule la variance d'α_sync.
    pub fn alpha_sync_variance(&self) -> f64 {
        if self.alpha_sync_samples.len() < 2 {
            return 0.0;
        }
        let mean: f64 =
            self.alpha_sync_samples.iter().sum::<f64>() / self.alpha_sync_samples.len() as f64;
        let variance: f64 = self
            .alpha_sync_samples
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>()
            / self.alpha_sync_samples.len() as f64;
        variance
    }

    /// Calcule le taux d'erreur global.
    pub fn error_rate(&self) -> f64 {
        if self.total_operations == 0 {
            return 0.0;
        }
        self.error_count as f64 / self.total_operations as f64
    }

    // ── Check complet ──────────────────────────────────────────────────

    /// Exécute un check complet + actions correctives.
    /// Retourne le rapport de santé.
    pub async fn check(&mut self, hub: &MemoryHub) -> HealthReport {
        self.check_count += 1;

        let avg_lat = self.avg_latency();
        let fail_rate = self.search_failure_rate();
        let entries = 0; // Hub ne fournit pas encore le count direct
        let compactions = self.compactions_last_hour();
        let msg_rate = self.message_rate();
        let ctx_estimate = self.current_context_estimate;
        let alpha_var = self.alpha_sync_variance();
        let err_rate = self.error_rate();

        let mut actions: Vec<String> = Vec::new();
        let mut is_fatigued = false;

        // Déterminer le statut
        let status = if compactions > self.config.max_compactions_per_hour {
            is_fatigued = true;
            HealthStatus::Fatigued(format!(
                "{} compactions/heure, α variance {:.3}",
                compactions, alpha_var
            ))
        } else if ctx_estimate > self.config.context_size_warn_tokens {
            is_fatigued = true;
            HealthStatus::Fatigued(format!("contexte estimé à {}K tokens", ctx_estimate / 1000))
        } else if avg_lat > self.config.latency_warn_ms {
            HealthStatus::Degraded(format!("latence élevée: {:.1}ms", avg_lat))
        } else if entries > self.config.max_entries_warn {
            HealthStatus::Degraded(format!("trop d'entrées: {}", entries))
        } else if fail_rate > self.config.search_failure_ratio {
            HealthStatus::FallbackActive
        } else if alpha_var > self.config.max_alpha_sync_variance {
            HealthStatus::Fatigued(format!("variance α élevée: {:.3}", alpha_var))
        } else if msg_rate > self.config.max_message_rate {
            HealthStatus::Degraded(format!("rythme élevé: {:.0} msg/min", msg_rate))
        } else if err_rate > self.config.max_error_rate {
            HealthStatus::Degraded(format!("taux d'erreur: {:.1}%", err_rate * 100.0))
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

        if is_fatigued {
            actions.push("ALERTE: fatigue cognitive détectée — une pause serait bénéfique".into());
            warn!("MemoryHealth: 🧠 fatigue cognitive — pause recommandée");
        }

        if compactions > 0 {
            actions.push(format!("compactions dernière heure: {}", compactions));
        }

        // Reset périodique des compteurs
        if self.check_count % 10 == 0 {
            self.search_attempts = 0;
            self.search_failures = 0;
            self.alpha_sync_samples.clear();
        }

        let report = HealthReport {
            avg_latency_ms: avg_lat,
            memory_entries: entries,
            search_success_rate: 1.0 - fail_rate,
            status: status.clone(),
            actions_taken: actions,
            compactions_last_hour: compactions,
            message_rate: msg_rate,
            estimated_context_tokens: ctx_estimate,
            alpha_sync_variance: alpha_var,
            error_rate: err_rate,
            is_fatigued,
        };
        self.last_report = Some(report.clone());

        // Log
        match &status {
            HealthStatus::Healthy => info!("MemoryHealth: ✅ santé OK"),
            HealthStatus::Degraded(s) => warn!("MemoryHealth: ⚠️ dégradé — {}", s),
            HealthStatus::FallbackActive => warn!("MemoryHealth: 🟡 fallback actif"),
            HealthStatus::Fatigued(s) => warn!("MemoryHealth: 🧠 fatigue — {}", s),
        }

        report
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

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

    #[test]
    fn test_compaction_tracking() {
        let mut health = MemoryHealth::new(HealthConfig::default());
        health.record_compaction();
        health.record_compaction();
        health.record_compaction();
        assert_eq!(health.compactions_last_hour(), 3);
    }

    #[test]
    fn test_message_rate() {
        let mut health = MemoryHealth::new(HealthConfig::default());
        health.record_message();
        health.record_message();
        health.record_message();
        assert_eq!(health.message_rate(), 3.0);
    }

    #[test]
    fn test_alpha_sync_variance() {
        let mut health = MemoryHealth::new(HealthConfig::default());
        health.record_alpha_sync(0.1);
        health.record_alpha_sync(0.5);
        health.record_alpha_sync(0.9);
        let var = health.alpha_sync_variance();
        assert!(var > 0.0);
    }

    #[test]
    fn test_error_rate() {
        let mut health = MemoryHealth::new(HealthConfig::default());
        health.record_operation(false);
        health.record_operation(true);
        health.record_operation(false);
        assert!((health.error_rate() - 1.0 / 3.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_fatigue_detection() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = crate::memory_hub::MemoryHub::new(dir.path()).await;
        let mut health = MemoryHealth::new(HealthConfig::default());

        // Simuler 3 compactions (seuil = 2)
        health.record_compaction();
        health.record_compaction();
        health.record_compaction();

        let report = health.check(&hub).await;
        assert!(report.is_fatigued);
        assert_eq!(report.compactions_last_hour, 3);
        match report.status {
            HealthStatus::Fatigued(_) => {}
            _ => panic!("devrait être Fatigued"),
        }
    }
}
