// ==========================================================================
// memory_health.rs — Surveillance proactive (V6.3, point 3.C du document)
//
// Moniteur de santé qui observe en continu plusieurs métriques de la
// mémoire et déclenche des actions correctives quand elles s'écartent
// des bornes sûres.
//
// MÉTRIQUES SURVEILLÉES
//
//   1. Taux d'occupation épisodique : len / capacity
//      → >90% → trigger prune aggressif
//   2. Hit rate cache prédictif : hits / (hits + misses) sur fenêtre récente
//      → < 5% → cache invalide, clear()
//   3. Latence retrieve : moyenne mobile des temps de recherche
//      → > seuil → compact() le HNSW pour rebuild propre
//   4. Croissance sémantique : taux d'ajout de prototypes / step
//      → > seuil → durcir le threshold de matching (évite l'inflation)
//   5. Confirmations rétro-causales : retro_bindings_total ratio
//      → 0 sur N steps → soit pas de prédictions, soit système figé
//
// ARCHITECTURE
//
//   - HealthMonitor maintient un état + une fenêtre glissante de mesures.
//   - .check() est appelé périodiquement et retourne un Status + une liste
//     d'Actions recommandées.
//   - Les actions sont déclaratives — l'appelant les applique selon sa
//     politique (auto-correct, log seulement, alerter humain, etc.).
//
// USAGE
//
//   let mut health = HealthMonitor::new();
//
//   // Dans la boucle
//   health.record_retrieve(latency_us);
//   health.record_cache(hits, misses);
//
//   if step % HEALTH_CHECK_EVERY == 0 {
//       let report = health.check(&memory, &pred_cache);
//       for action in &report.actions {
//           match action {
//               HealthAction::PruneAggressive => { ... },
//               HealthAction::ClearCache => { pred_cache.clear(); },
//               ...
//           }
//       }
//   }
// ==========================================================================

use std::collections::VecDeque;
use crate::memory::AtemporalMemory;
use crate::predictive_cache::PredictiveCache;

// --------------------------------------------------------------------------
// Configuration
// --------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HealthThresholds {
    /// Si len(episodic) / capacity > ce ratio → prune.
    pub episodic_full_ratio: f64,
    /// Si hit_rate du cache prédictif < ce ratio → clear.
    pub cache_hit_rate_min: f64,
    /// Si latence moyenne (µs) du retrieve > ce seuil → compact HNSW.
    pub retrieve_latency_max_us: f64,
    /// Si nouveau prototype sémantique / step > ce taux → durcir threshold.
    pub semantic_growth_rate_max: f64,
    /// Fenêtre (steps) pour les moyennes glissantes.
    pub window_size: usize,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            episodic_full_ratio: 0.95,       // 90 → 95 % (full normal en cruise)
            cache_hit_rate_min: 0.05,
            retrieve_latency_max_us: 500.0,
            semantic_growth_rate_max: 1.5,   // 0.5 → 1.5 (croissance précoce normale)
            window_size: 64,
        }
    }
}

// --------------------------------------------------------------------------
// Status global
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Toutes les métriques dans les bornes.
    Healthy,
    /// Une métrique au-dessus du seuil mais pas critique.
    Degraded,
    /// Au moins une métrique critique.
    Critical,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthAction {
    PruneAggressive,
    ClearCache,
    CompactHnsw,
    HardenSemanticThreshold,
    NoOp,
}

#[derive(Debug, Clone)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub episodic_fill_ratio: f64,
    pub cache_hit_rate: f64,
    pub retrieve_latency_avg_us: f64,
    pub semantic_growth_rate: f64,
    pub actions: Vec<HealthAction>,
    pub notes: Vec<String>,
}

// --------------------------------------------------------------------------
// HealthMonitor
// --------------------------------------------------------------------------

pub struct HealthMonitor {
    pub thresholds: HealthThresholds,
    /// Latences retrieve mesurées (en µs).
    retrieve_latencies: VecDeque<f64>,
    /// Snapshots du nombre de prototypes sémantiques.
    semantic_snapshots: VecDeque<(u64, usize)>,
    /// Snapshot du dernier check pour calculs incrémentaux.
    pub last_hits: u64,
    pub last_misses: u64,
    /// Compteur de checks effectués.
    pub check_count: u64,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self::with_thresholds(HealthThresholds::default())
    }

    pub fn with_thresholds(thresholds: HealthThresholds) -> Self {
        Self {
            thresholds,
            retrieve_latencies: VecDeque::with_capacity(64),
            semantic_snapshots: VecDeque::with_capacity(64),
            last_hits: 0,
            last_misses: 0,
            check_count: 0,
        }
    }

    pub fn record_retrieve_latency(&mut self, us: f64) {
        if self.retrieve_latencies.len() >= self.thresholds.window_size {
            self.retrieve_latencies.pop_front();
        }
        self.retrieve_latencies.push_back(us);
    }

    /// Snapshot périodique de l'évolution de la mémoire sémantique
    /// (pour calculer le taux de croissance).
    pub fn snapshot_semantic(&mut self, current_t: u64, n_prototypes: usize) {
        if self.semantic_snapshots.len() >= self.thresholds.window_size {
            self.semantic_snapshots.pop_front();
        }
        self.semantic_snapshots.push_back((current_t, n_prototypes));
    }

    /// Évalue toutes les métriques et propose des actions.
    pub fn check(&mut self, memory: &AtemporalMemory,
                 cache: &PredictiveCache) -> HealthReport
    {
        self.check_count += 1;
        let mut actions = Vec::new();
        let mut notes = Vec::new();
        let mut critical = false;
        let mut degraded = false;

        // ---- 1. Taux d'occupation épisodique ----
        let cap = memory.episodic.capacity.max(1);
        let fill = memory.episodic.traces.len() as f64 / cap as f64;
        if fill > self.thresholds.episodic_full_ratio {
            actions.push(HealthAction::PruneAggressive);
            notes.push(format!("episodic at {:.0}% capacity → prune", fill * 100.0));
            // Critical seulement si vraiment saturé (≥ 99.9%)
            if fill >= 0.999 { critical = true; } else { degraded = true; }
        }

        // ---- 2. Hit rate cache ----
        let total = cache.hits + cache.misses;
        let hit_rate = if total > 0 { cache.hits as f64 / total as f64 } else { 1.0 };
        if total > 32 && hit_rate < self.thresholds.cache_hit_rate_min {
            actions.push(HealthAction::ClearCache);
            notes.push(format!("cache hit rate {:.1}% < min → clear",
                               hit_rate * 100.0));
            degraded = true;
        }

        // ---- 3. Latence retrieve ----
        let lat_avg = if self.retrieve_latencies.is_empty() {
            0.0
        } else {
            self.retrieve_latencies.iter().sum::<f64>()
                / self.retrieve_latencies.len() as f64
        };
        if lat_avg > self.thresholds.retrieve_latency_max_us {
            actions.push(HealthAction::CompactHnsw);
            notes.push(format!("retrieve avg {:.0}µs > {:.0}µs → compact",
                               lat_avg, self.thresholds.retrieve_latency_max_us));
            degraded = true;
        }

        // ---- 4. Croissance sémantique ----
        let growth = self.semantic_growth_rate();
        if growth > self.thresholds.semantic_growth_rate_max {
            actions.push(HealthAction::HardenSemanticThreshold);
            notes.push(format!("semantic growth {:.2}/step > max → harden",
                               growth));
            degraded = true;
        }

        // ---- Status global ----
        let status = if critical { HealthStatus::Critical }
                     else if degraded { HealthStatus::Degraded }
                     else { HealthStatus::Healthy };

        if actions.is_empty() {
            actions.push(HealthAction::NoOp);
        }

        HealthReport {
            status,
            episodic_fill_ratio: fill,
            cache_hit_rate: hit_rate,
            retrieve_latency_avg_us: lat_avg,
            semantic_growth_rate: growth,
            actions,
            notes,
        }
    }

    /// Croissance instantanée : (n_proto_recent - n_proto_old) / Δt_steps.
    fn semantic_growth_rate(&self) -> f64 {
        if self.semantic_snapshots.len() < 2 { return 0.0; }
        let first = self.semantic_snapshots.front().unwrap();
        let last = self.semantic_snapshots.back().unwrap();
        let dt = last.0.saturating_sub(first.0) as f64;
        if dt < 1.0 { return 0.0; }
        (last.1 as f64 - first.1 as f64) / dt
    }
}

impl Default for HealthMonitor {
    fn default() -> Self { Self::new() }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::predictive_cache::PredictiveCache;

    #[test]
    fn healthy_state_no_actions() {
        let mut monitor = HealthMonitor::new();
        let memory = AtemporalMemory::new(8, 64);
        let cache = PredictiveCache::new(16, 0.92);
        let report = monitor.check(&memory, &cache);
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.actions, vec![HealthAction::NoOp]);
    }

    #[test]
    fn detects_episodic_full() {
        let mut monitor = HealthMonitor::new();
        let mut memory = AtemporalMemory::new(4, 10);
        // Remplit à 100%
        for i in 0..10 {
            memory.episodic.push(vec![0.0; 4], i, None);
        }
        let cache = PredictiveCache::new(16, 0.92);
        let report = monitor.check(&memory, &cache);
        assert_eq!(report.status, HealthStatus::Critical);
        assert!(report.actions.contains(&HealthAction::PruneAggressive));
    }

    #[test]
    fn detects_low_cache_hit_rate() {
        let mut monitor = HealthMonitor::new();
        let memory = AtemporalMemory::new(8, 64);
        let mut cache = PredictiveCache::new(16, 0.92);
        // Force misses (queries orthogonales aux clés)
        cache.put(vec![1.0, 0.0, 0.0, 0.0], vec![]);
        for _ in 0..40 {
            let _ = cache.get(&[0.0, 1.0, 0.0, 0.0]);  // miss à chaque fois
        }
        let report = monitor.check(&memory, &cache);
        assert!(report.actions.contains(&HealthAction::ClearCache));
    }

    #[test]
    fn detects_high_retrieve_latency() {
        let mut monitor = HealthMonitor::new();
        for _ in 0..10 {
            monitor.record_retrieve_latency(1000.0);  // > 500 µs threshold
        }
        let memory = AtemporalMemory::new(8, 64);
        let cache = PredictiveCache::new(16, 0.92);
        let report = monitor.check(&memory, &cache);
        assert!(report.actions.contains(&HealthAction::CompactHnsw));
    }

    #[test]
    fn growth_rate_is_zero_initially() {
        let mut monitor = HealthMonitor::new();
        monitor.snapshot_semantic(10, 5);
        monitor.snapshot_semantic(20, 5);
        assert!((monitor.semantic_growth_rate() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn growth_rate_detected() {
        let mut monitor = HealthMonitor::new();
        monitor.snapshot_semantic(10, 0);
        monitor.snapshot_semantic(20, 10);
        // 10 nouveaux prototypes en 10 steps → growth rate = 1.0
        assert!((monitor.semantic_growth_rate() - 1.0).abs() < 1e-6);
    }
}
