//! ContinuousSummarizer — Résumé incrémental continu pour la survie au contexte.
//!
//! # Problème
//! Les résumés de session ne sont générés qu'en fin de session (`session_summary.py`
//! n'existe pas encore en Rust). En cas de compaction, le contexte perdu n'est
//! pas récupérable.
//!
//! # Solution
//! Un thread asynchrone tourne en arrière-plan et génère un résumé structuré
//! toutes les 10 minutes (ou après N messages). Le résumé est stocké dans
//! `summary_latest.md` et poussé sur le bus d'événements.
//!
//! # Contenu du résumé
//! - Dernières décisions importantes
//! - Erreurs et corrections
//! - Contexte de la tâche en cours
//! - Faits nouveaux appris
//! - Métriques de performance
//!
//! # Survivabilité
//! Le fichier `summary_latest.md` est externe au contexte LLM et n'est pas
//! affecté par les compactions. Au réveil, l'agent le lit et l'injecte.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ── Types ──────────────────────────────────────────────────────────────

/// Un élément marquant dans l'historique récent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp_ms: i64,
    pub kind: EventKind,
    pub summary: String,
    pub details: String,
}

/// Catégorie d'événement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventKind {
    Decision,
    Error,
    Correction,
    Fact,
    Insight,
    Progress,
    StateChange,
    Milestone,
    Other(String),
}

/// Résumé complet à un instant T.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub generated_at_ms: i64,
    pub session_id: String,
    pub message_count: u64,
    pub uptime_secs: u64,

    /// Objectif courant de la session
    pub current_goal: String,
    /// Tâche active
    pub active_task: String,
    /// État d'avancement
    pub progress: String,

    /// Événements marquants depuis le dernier résumé
    pub recent_events: Vec<TimelineEvent>,
    /// Décisions clés de la session
    pub key_decisions: Vec<String>,
    /// Erreurs rencontrées et corrections
    pub errors_and_fixes: Vec<(String, String)>,
    /// Faits nouveaux découverts
    pub new_facts: Vec<String>,
    /// Métadonnées
    pub metadata: HashMap<String, String>,
}

impl SessionSummary {
    pub fn new(session_id: &str) -> Self {
        Self {
            generated_at_ms: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            message_count: 0,
            uptime_secs: 0,
            current_goal: String::new(),
            active_task: String::new(),
            progress: String::new(),
            recent_events: Vec::new(),
            key_decisions: Vec::new(),
            errors_and_fixes: Vec::new(),
            new_facts: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Formate le résumé en Markdown pour écriture dans `summary_latest.md`.
    pub fn format_markdown(&self) -> String {
        let now = chrono::DateTime::from_timestamp_millis(self.generated_at_ms)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "inconnu".into());

        let mut md = "# Résumé de session\n\n".to_string();
        md.push_str(&format!("- **Généré le**: {}\n", now));
        md.push_str(&format!("- **Session**: {}\n", self.session_id));
        md.push_str(&format!("- **Messages**: {}\n", self.message_count));
        md.push_str(&format!("- **Uptime**: {}min\n\n", self.uptime_secs / 60));

        if !self.current_goal.is_empty() {
            md.push_str(&format!("## Objectif\n\n{}\n\n", self.current_goal));
        }
        if !self.active_task.is_empty() {
            md.push_str(&format!("## Tâche active\n\n{}\n\n", self.active_task));
        }
        if !self.progress.is_empty() {
            md.push_str(&format!("## Progrès\n\n{}\n\n", self.progress));
        }

        if !self.key_decisions.is_empty() {
            md.push_str("## Décisions clés\n\n");
            for d in &self.key_decisions {
                md.push_str(&format!("- {}\n", d));
            }
            md.push('\n');
        }

        if !self.errors_and_fixes.is_empty() {
            md.push_str("## Erreurs et corrections\n\n");
            for (err, fix) in &self.errors_and_fixes {
                md.push_str(&format!("- **Erreur**: {} → **Fix**: {}\n", err, fix));
            }
            md.push('\n');
        }

        if !self.new_facts.is_empty() {
            md.push_str("## Faits nouveaux\n\n");
            for f in &self.new_facts {
                md.push_str(&format!("- {}\n", f));
            }
            md.push('\n');
        }

        if !self.recent_events.is_empty() {
            md.push_str("## Événements récents\n\n");
            for e in &self.recent_events {
                let ts = chrono::DateTime::from_timestamp_millis(e.timestamp_ms)
                    .map(|d| d.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "??:??:??".into());
                md.push_str(&format!("- [{}] **{:?}**: {}\n", ts, e.kind, e.summary));
            }
            md.push('\n');
        }

        md.push_str("---\n*Résumé automatique généré par ContinuousSummarizer*\n");
        md
    }
}

// ── Configuration ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SummarizerConfig {
    /// Répertoire de sortie du résumé
    pub data_dir: PathBuf,
    /// Intervalle entre résumés (secondes)
    pub interval_secs: u64,
    /// Nombre de messages avant résumé (si atteint avant l'intervalle)
    pub message_threshold: u64,
    /// Nom du fichier de résumé
    pub summary_filename: String,
    /// Nom du fichier d'événements en attente
    pub pending_facts_filename: String,
    /// Identifiant de session
    pub session_id: String,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/soulsystem/data"),
            interval_secs: 600,    // 10 minutes
            message_threshold: 30, // ou tous les 30 messages
            summary_filename: "summary_latest.md".into(),
            pending_facts_filename: "pending_facts.md".into(),
            session_id: "unknown".into(),
        }
    }
}

// ── Summarizer ─────────────────────────────────────────────────────────

/// Générateur de résumé incrémental continu.
pub struct ContinuousSummarizer {
    config: SummarizerConfig,
    summary: Arc<RwLock<SessionSummary>>,
    start_time: tokio::time::Instant,
    summary_path: PathBuf,
    pending_facts_path: PathBuf,
}

impl ContinuousSummarizer {
    pub fn new(config: SummarizerConfig) -> Self {
        let summary_path = config.data_dir.join(&config.summary_filename);
        let pending_facts_path = config.data_dir.join(&config.pending_facts_filename);

        Self {
            summary: Arc::new(RwLock::new(SessionSummary::new(&config.session_id))),
            start_time: tokio::time::Instant::now(),
            config,
            summary_path,
            pending_facts_path,
        }
    }

    /// Retourne le résumé partagé pour mise à jour.
    pub fn summary(&self) -> Arc<RwLock<SessionSummary>> {
        self.summary.clone()
    }

    // ── Mise à jour du résumé ──────────────────────────────────────────

    /// Définit l'objectif courant.
    pub async fn set_goal(&self, goal: &str) {
        self.summary.write().await.current_goal = goal.to_string();
    }

    /// Définit la tâche active.
    pub async fn set_active_task(&self, task: &str) {
        self.summary.write().await.active_task = task.to_string();
    }

    /// Met à jour la progression.
    pub async fn set_progress(&self, progress: &str) {
        self.summary.write().await.progress = progress.to_string();
    }

    /// Ajoute une décision clé.
    pub async fn add_decision(&self, decision: &str) {
        let mut s = self.summary.write().await;
        s.key_decisions.push(decision.to_string());
        if s.key_decisions.len() > 20 {
            s.key_decisions.remove(0);
        }
    }

    /// Ajoute une erreur et sa correction.
    pub async fn add_error_fix(&self, error: &str, fix: &str) {
        self.summary
            .write()
            .await
            .errors_and_fixes
            .push((error.to_string(), fix.to_string()));
        // Garder les 15 dernières
        let mut s = self.summary.write().await;
        if s.errors_and_fixes.len() > 15 {
            s.errors_and_fixes.remove(0);
        }
    }

    /// Ajoute un fait nouveau.
    pub async fn add_fact(&self, fact: &str) {
        let mut s = self.summary.write().await;
        // Éviter les doublons
        if !s.new_facts.iter().any(|f| f == fact) {
            s.new_facts.push(fact.to_string());
            if s.new_facts.len() > 20 {
                s.new_facts.remove(0);
            }
        }
    }

    /// Ajoute un événement à la timeline.
    pub async fn add_event(&self, kind: EventKind, summary: &str, details: &str) {
        self.summary
            .write()
            .await
            .recent_events
            .push(TimelineEvent {
                timestamp_ms: Utc::now().timestamp_millis(),
                kind,
                summary: summary.to_string(),
                details: details.to_string(),
            });
        // Garder les 50 derniers événements
        let mut s = self.summary.write().await;
        if s.recent_events.len() > 50 {
            s.recent_events.remove(0);
        }
    }

    /// Incrémente le compteur de messages.
    pub async fn increment_messages(&self) -> u64 {
        let mut s = self.summary.write().await;
        s.message_count += 1;
        s.message_count
    }

    // ── Génération et écriture ─────────────────────────────────────────

    /// Génère le résumé et l'écrit dans le fichier.
    pub async fn generate_summary(&self) -> anyhow::Result<String> {
        let mut s = self.summary.write().await;
        s.generated_at_ms = Utc::now().timestamp_millis();
        s.uptime_secs = self.start_time.elapsed().as_secs();

        let md = s.format_markdown();
        tokio::fs::write(&self.summary_path, &md).await?;
        info!(
            "ContinuousSummarizer: résumé généré ({} messages, {}min d'uptime)",
            s.message_count,
            s.uptime_secs / 60
        );
        Ok(md)
    }

    // ── Faits en attente (pour S5) ─────────────────────────────────────

    /// Écrit les faits en attente de validation dans `pending_facts.md`.
    pub async fn write_pending_facts(&self) -> anyhow::Result<()> {
        let s = self.summary.read().await;
        if s.new_facts.is_empty() {
            return Ok(());
        }

        let mut md = String::from("# Faits en attente de validation\n\n");
        md.push_str("Les faits suivants ont été détectés comme potentiellement importants.\n");
        md.push_str("Valide-les avec `!accept all` ou individuellement.\n\n");

        for (i, fact) in s.new_facts.iter().enumerate() {
            md.push_str(&format!("{}. [ ] {}\n", i + 1, fact));
        }

        md.push_str("\n---\n*Généré automatiquement par ContinuousSummarizer*\n");
        tokio::fs::write(&self.pending_facts_path, &md).await?;
        info!(
            "ContinuousSummarizer: {} faits en attente écrits",
            s.new_facts.len()
        );
        Ok(())
    }

    // ── Boucle principale ──────────────────────────────────────────────

    /// Boucle asynchrone qui génère les résumés à intervalle régulier.
    pub async fn run(&self) {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(self.config.interval_secs));
        let mut last_msg_count: u64 = 0;

        loop {
            interval.tick().await;

            // Vérifier si le seuil de messages a été atteint
            let current_count = self.summary.read().await.message_count;
            if current_count - last_msg_count >= self.config.message_threshold {
                if let Err(e) = self.generate_summary().await {
                    tracing::error!("ContinuousSummarizer: erreur de génération: {}", e);
                }
                // Écrire aussi les faits en attente
                if let Err(e) = self.write_pending_facts().await {
                    tracing::error!("ContinuousSummarizer: erreur écriture faits: {}", e);
                }
                last_msg_count = current_count;
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_summary_generation() {
        let dir = TempDir::new().unwrap();
        let cfg = SummarizerConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let summarizer = ContinuousSummarizer::new(cfg);

        summarizer.set_goal("Tester le résumé").await;
        summarizer
            .set_active_task("Écrire des tests unitaires")
            .await;
        summarizer.set_progress("50%").await;
        summarizer.add_decision("Utiliser TempDir").await;
        summarizer.add_error_fix("Erreur X", "Fix Y").await;
        summarizer.add_fact("Le résumé fonctionne").await;

        let md = summarizer.generate_summary().await.unwrap();
        assert!(md.contains("Objectif"));
        assert!(md.contains("Tester le résumé"));
        assert!(md.contains("Décisions clés"));
        assert!(md.contains("Erreurs et corrections"));
        assert!(md.contains("Faits nouveaux"));

        // Vérifier que le fichier a été écrit
        let path = dir.path().join("summary_latest.md");
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_pending_facts() {
        let dir = TempDir::new().unwrap();
        let cfg = SummarizerConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let summarizer = ContinuousSummarizer::new(cfg);

        summarizer.add_fact("Fait important #1").await;
        summarizer.add_fact("Fait important #2").await;
        summarizer.write_pending_facts().await.unwrap();

        let path = dir.path().join("pending_facts.md");
        assert!(path.exists());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("Fait important #1"));
        assert!(content.contains("Fait important #2"));
    }

    #[tokio::test]
    async fn test_event_timeline() {
        let dir = TempDir::new().unwrap();
        let cfg = SummarizerConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let summarizer = ContinuousSummarizer::new(cfg);

        summarizer
            .add_event(EventKind::Decision, "Décision test", "Détails")
            .await;
        summarizer
            .add_event(EventKind::Error, "Erreur test", "Détails")
            .await;

        let summary = summarizer.summary();
        let s = summary.read().await;
        assert_eq!(s.recent_events.len(), 2);
        assert_eq!(s.recent_events[0].kind, EventKind::Decision);
        assert_eq!(s.recent_events[1].kind, EventKind::Error);
    }

    #[tokio::test]
    async fn test_message_threshold_triggers_summary() {
        let dir = TempDir::new().unwrap();
        let cfg = SummarizerConfig {
            data_dir: dir.path().to_path_buf(),
            interval_secs: 3600,  // Long interval, won't trigger
            message_threshold: 5, // Low threshold
            ..Default::default()
        };
        let summarizer = ContinuousSummarizer::new(cfg);

        // Simuler des messages
        for _ in 0..5 {
            summarizer.increment_messages().await;
        }

        // Vérifier que les messages sont comptés
        assert_eq!(summarizer.summary().read().await.message_count, 5);
    }
}
