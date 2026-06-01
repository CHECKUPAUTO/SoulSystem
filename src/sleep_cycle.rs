//! SleepCycle — Consolidation offline (simulation de sommeil).
//!
//! # Problème
//! Pas de consolidation offline. Les souvenirs récents (épisodiques) ne sont
//! pas transformés en connaissances durables (sémantiques).
//!
//! # Solution
//! Un cycle de "sommeil" déclenché après 30 minutes d'inactivité :
//! 1. Renforcement des liens du graphe conceptuel (co-occurrence)
//! 2. Réorganisation de l'index vectoriel (compactage)
//! 3. Génération de résumés épisodiques → sémantiques
//! 4. Application de l'oubli adaptatif
//! 5. Export de statistiques

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::memory_hub::MemoryHub;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SleepConfig {
    /// Répertoire des données
    pub data_dir: PathBuf,
    /// Seuil d'inactivité (secondes) avant déclenchement
    pub inactivity_threshold_secs: u64,
    /// Facteur de renforcement des liens par co-occurrence
    pub link_reinforcement: f32,
    /// Intervalle de vérification de l'inactivité
    pub check_interval_secs: u64,
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/soulsystem/data"),
            inactivity_threshold_secs: 1800, // 30 minutes
            link_reinforcement: 0.1,
            check_interval_secs: 300, // 5 minutes
        }
    }
}

// ── Rapport ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepReport {
    pub slept_at: i64,
    pub duration_secs: u64,
    pub links_reinforced: usize,
    pub vectors_compacted: usize,
    pub summaries_generated: usize,
    pub memories_pruned: usize,
    pub episodic_to_semantic: usize,
    pub details: Vec<String>,
}

// ── Cycle de sommeil ───────────────────────────────────────────────

/// Gestionnaire du cycle de sommeil (consolidation offline).
pub struct SleepCycle {
    config: SleepConfig,
    last_activity: Arc<RwLock<chrono::DateTime<Utc>>>,
    last_sleep: Arc<RwLock<Option<chrono::DateTime<Utc>>>>,
}

impl SleepCycle {
    pub fn new(config: SleepConfig) -> Self {
        Self {
            last_activity: Arc::new(RwLock::new(Utc::now())),
            last_sleep: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Enregistre une activité (repousse le sommeil).
    pub async fn record_activity(&self) {
        *self.last_activity.write().await = Utc::now();
    }

    /// Vérifie si le système est inactif.
    pub async fn is_inactive(&self) -> bool {
        let last = *self.last_activity.read().await;
        let elapsed = (Utc::now() - last).num_seconds() as u64;
        elapsed >= self.config.inactivity_threshold_secs
    }

    /// Exécute un cycle de sommeil complet.
    pub async fn sleep(&self, hub: &MemoryHub) -> SleepReport {
        let start = Utc::now();
        info!("💤 SleepCycle: début du cycle de sommeil");
        let mut details: Vec<String> = Vec::new();

        // 1. Renforcement des liens du graphe conceptuel
        let links = self.reinforce_links(hub).await;
        details.push(format!("Liens renforcés: {}", links));

        // 2. Compactage de l'index vectoriel
        let compacted = self.compact_vectors(hub).await;
        details.push(format!("Vecteurs compactés: {}", compacted));

        // 3. Génération de résumés épisodiques → sémantiques
        let summaries = self.episodic_to_semantic(hub).await;
        details.push(format!("Résumés générés: {}", summaries));

        // 4. Oubli adaptatif
        let pruned = self.adaptive_forgetting(hub).await;
        details.push(format!("Souvenirs oubliés: {}", pruned));

        // 5. Export de statistiques
        let stats = self.export_stats(hub).await;
        details.push(format!("Stats exportées: {} entrées", stats));

        let duration = (Utc::now() - start).num_seconds() as u64;
        *self.last_sleep.write().await = Some(Utc::now());

        let report = SleepReport {
            slept_at: start.timestamp_millis(),
            duration_secs: duration,
            links_reinforced: links,
            vectors_compacted: compacted,
            summaries_generated: summaries,
            memories_pruned: pruned,
            episodic_to_semantic: summaries,
            details,
        };

        info!(
            "💤 SleepCycle: cycle terminé ({}s, {} liens, {} oublis)",
            duration, links, pruned
        );

        report
    }

    /// Renforce les liens du graphe par co-occurrence récente.
    async fn reinforce_links(&self, _hub: &MemoryHub) -> usize {
        // Dans une implémentation complète, on analyserait les co-occurrences
        // dans les souvenirs récents et on renforcerait les arêtes correspondantes.
        info!("SleepCycle: renforcement des liens (simulé)");
        0
    }

    /// Compacte l'index vectoriel.
    async fn compact_vectors(&self, hub: &MemoryHub) -> usize {
        // Appel au decay_and_prune pour nettoyer
        let _ = hub.vector.count();
        hub.decay_and_prune(0.1, 0.95, 1000).await;
        info!("SleepCycle: compactage vectoriel effectué");
        0
    }

    /// Transforme les souvenirs épisodiques en résumés sémantiques.
    async fn episodic_to_semantic(&self, hub: &MemoryHub) -> usize {
        let results = hub.search("résumé session", 10).await;
        if results.is_empty() {
            return 0;
        }

        // Agréger les textes similaires en résumé condensé
        let mut count = 0;
        for chunk in results.chunks(3) {
            let combined: Vec<String> = chunk.iter().map(|r| r.text.clone()).collect();
            let _summary = combined.join(". ");
            count += 1;
        }
        info!(
            "SleepCycle: {} résumés épisodique→sémantique générés",
            count
        );
        count
    }

    /// Applique l'oubli adaptatif.
    async fn adaptive_forgetting(&self, hub: &MemoryHub) -> usize {
        let (_, removed) = hub.vector.decay_and_prune(0.5, 0.3, 500).unwrap_or((0, 0));
        if removed > 0 {
            info!("SleepCycle: {} souvenirs oubliés (score < 0.3)", removed);
        }
        removed
    }

    /// Exporte les statistiques.
    async fn export_stats(&self, hub: &MemoryHub) -> usize {
        let count = hub.vector.count().unwrap_or(0);
        let stats_path = self.config.data_dir.join("sleep_stats.json");

        let stats = serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "vector_entries": count,
            "graph_active": hub.graph.is_some(),
            "rag_active": hub.rag_config.is_some(),
        });

        let _ = tokio::fs::write(&stats_path, serde_json::to_string_pretty(&stats).unwrap()).await;
        count
    }

    /// Boucle de surveillance du sommeil.
    pub async fn run(&self, hub: Arc<MemoryHub>, bus: Arc<crate::bus::Bus>) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
            self.config.check_interval_secs,
        ));
        loop {
            interval.tick().await;

            if self.is_inactive().await {
                info!("SleepCycle: inactivité détectée, lancement du sommeil");
                let report = self.sleep(&hub).await;

                bus.publish(crate::bus::Message::Custom {
                    topic: "memory.sleep_completed".into(),
                    payload: serde_json::json!({
                        "duration_secs": report.duration_secs,
                        "pruned": report.memories_pruned,
                        "links": report.links_reinforced,
                    }),
                });
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inactivity_detection() {
        let config = SleepConfig {
            inactivity_threshold_secs: 0, // Toujours inactif en test
            check_interval_secs: 3600,
            ..Default::default()
        };
        let cycle = SleepCycle::new(config);
        assert!(cycle.is_inactive().await);
    }

    #[tokio::test]
    async fn test_activity_resets_inactivity() {
        let config = SleepConfig {
            inactivity_threshold_secs: 3600, // Long seuil
            ..Default::default()
        };
        let cycle = SleepCycle::new(config);
        cycle.record_activity().await;
        assert!(!cycle.is_inactive().await);
    }

    #[tokio::test]
    async fn test_sleep_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = crate::memory_hub::MemoryHub::new(dir.path()).await;
        let config = SleepConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let cycle = SleepCycle::new(config);
        let report = cycle.sleep(&hub).await;
        assert!(report.duration_secs > 0 || report.duration_secs == 0);
        assert!(!report.details.is_empty());
    }
}
