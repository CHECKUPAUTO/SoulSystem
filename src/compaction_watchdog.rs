//! CompactionWatchdog — Interception et maîtrise des compactions OpenClaw.
//!
//! # Problème
//! OpenClaw force des compactions bien avant la limite de 1M tokens du modèle,
//! causant une perte brutale du contexte et une dégradation en cascade.
//!
//! # Solution
//! 1. **Sauvegarde proactive** : avant chaque compaction, l'état courant (tâche,
//!    décisions récentes, contexte) est sérialisé dans `last_compaction_state.json`.
//! 2. **Restauration** : après compaction, le premier cycle injecte le condensé
//!    dans le nouveau contexte via un fichier `context_restore.md`.
//! 3. **Surveillance** : le watchdog détecte les compactions via un fichier
//!    trigger (`compaction_trigger.flag`) ou une estimation de taille de contexte.
//!
//! # Utilisation
//! Le watchdog est lancé comme une tâche tokio dans `main.rs` et expose une API
//! pour sauvegarder/restaurer l'état à la demande.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ── État du contexte ───────────────────────────────────────────────────

/// État complet du contexte à un instant T, prêt pour sérialisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextState {
    /// Timestamp de la sauvegarde (Unix millis)
    pub saved_at_ms: i64,
    /// Résumé courant de la tâche en cours
    pub current_task: String,
    /// Dernières décisions importantes
    pub recent_decisions: Vec<String>,
    /// Erreurs récentes et corrections
    pub recent_errors: Vec<String>,
    /// Objectif global de la session
    pub session_goal: String,
    /// Prochaines étapes prévues
    pub next_steps: Vec<String>,
    /// Nombre de messages échangés depuis le dernier reset
    pub message_count: u64,
    /// Métadonnées additionnelles (version, source, etc.)
    pub metadata: std::collections::HashMap<String, String>,
}

impl ContextState {
    pub fn new() -> Self {
        Self {
            saved_at_ms: Utc::now().timestamp_millis(),
            current_task: String::new(),
            recent_decisions: Vec::new(),
            recent_errors: Vec::new(),
            session_goal: String::new(),
            next_steps: Vec::new(),
            message_count: 0,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Formate l'état en texte injectable dans le contexte.
    pub fn format_for_context(&self) -> String {
        let mut buf = String::from("## État précédent (récupéré après compaction)\n\n");
        buf.push_str(&format!(
            "**Sauvegardé le**: {}\n\n",
            chrono::DateTime::from_timestamp_millis(self.saved_at_ms)
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "inconnu".into())
        ));

        if !self.session_goal.is_empty() {
            buf.push_str(&format!("**Objectif**: {}\n\n", self.session_goal));
        }
        if !self.current_task.is_empty() {
            buf.push_str(&format!("**Tâche en cours**: {}\n\n", self.current_task));
        }
        if !self.recent_decisions.is_empty() {
            buf.push_str("**Décisions récentes**:\n");
            for d in &self.recent_decisions {
                buf.push_str(&format!("- {}\n", d));
            }
            buf.push('\n');
        }
        if !self.recent_errors.is_empty() {
            buf.push_str("**Erreurs corrigées**:\n");
            for e in &self.recent_errors {
                buf.push_str(&format!("- {}\n", e));
            }
            buf.push('\n');
        }
        if !self.next_steps.is_empty() {
            buf.push_str("**Prochaines étapes**:\n");
            for s in &self.next_steps {
                buf.push_str(&format!("- {}\n", s));
            }
            buf.push('\n');
        }
        buf.push_str("---\n*Reprends ton travail là où tu t'es arrêté.*\n");
        buf
    }
}

impl Default for ContextState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Configuration ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Répertoire où stocker les fichiers d'état
    pub data_dir: PathBuf,
    /// Seuil de messages avant déclenchement préventif
    pub message_threshold: u64,
    /// Intervalle de vérification (secondes)
    pub check_interval_secs: u64,
    /// Activer la sauvegarde préventive
    pub proactive_save: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/soulsystem/data"),
            message_threshold: 50,
            check_interval_secs: 30,
            proactive_save: true,
        }
    }
}

// ── Watchdog ───────────────────────────────────────────────────────────

/// Watchdog de compaction.
///
/// # Responsabilités
/// - Maintenir un état courant du contexte (`ContextState`)
/// - Sauvegarder cet état avant compaction
/// - Restaurer l'état après compaction
/// - Détecter les signaux de compaction (trigger file, logs)
pub struct CompactionWatchdog {
    config: CompactionConfig,
    state: Arc<RwLock<ContextState>>,
    compaction_count: Arc<RwLock<u64>>,
    trigger_path: PathBuf,
    state_path: PathBuf,
    restore_path: PathBuf,
}

impl CompactionWatchdog {
    pub fn new(config: CompactionConfig) -> Self {
        let data_dir = &config.data_dir;
        let trigger_path = data_dir.join("compaction_trigger.flag");
        let state_path = data_dir.join("last_compaction_state.json");
        let restore_path = data_dir.join("context_restore.md");

        Self {
            config,
            state: Arc::new(RwLock::new(ContextState::new())),
            compaction_count: Arc::new(RwLock::new(0)),
            trigger_path,
            state_path,
            restore_path,
        }
    }

    /// Retourne l'état partagé pour mise àjour par le système
    pub fn state(&self) -> Arc<RwLock<ContextState>> {
        self.state.clone()
    }

    /// Retourne le compteur de compactions
    pub fn compaction_count(&self) -> Arc<RwLock<u64>> {
        self.compaction_count.clone()
    }

    // ── Sauvegarde ──────────────────────────────────────────────────────

    /// Sauvegarde l'état courant dans le fichier JSON.
    pub async fn save_state(&self) -> anyhow::Result<()> {
        let state = self.state.read().await;
        let json = serde_json::to_string_pretty(&*state)?;
        tokio::fs::write(&self.state_path, &json).await?;

        // Générer aussi le fichier de restauration pour injection context
        let restore_text = state.format_for_context();
        tokio::fs::write(&self.restore_path, &restore_text).await?;

        info!(
            "CompactionWatchdog: état sauvegardé ({} décisions, {} erreurs)",
            state.recent_decisions.len(),
            state.recent_errors.len()
        );
        Ok(())
    }

    /// Sauvegarde et incrémente le compteur de compaction.
    pub async fn on_compaction(&self) -> anyhow::Result<()> {
        self.save_state().await?;
        let mut count = self.compaction_count.write().await;
        *count += 1;
        info!("CompactionWatchdog: compaction #{} enregistrée", *count);
        Ok(())
    }

    // ── Restauration ────────────────────────────────────────────────────

    /// Vérifie si un état précédent existe et le retourne.
    pub async fn load_previous_state(&self) -> anyhow::Result<Option<ContextState>> {
        match tokio::fs::read_to_string(&self.state_path).await {
            Ok(json) => {
                let state: ContextState = serde_json::from_str(&json)?;
                info!(
                    "CompactionWatchdog: état précédent restauré (sauvé le {})",
                    chrono::DateTime::from_timestamp_millis(state.saved_at_ms)
                        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "inconnu".into())
                );
                Ok(Some(state))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Retourne le texte de restauration pour injection dans le contexte.
    pub async fn get_restore_text(&self) -> String {
        match tokio::fs::read_to_string(&self.restore_path).await {
            Ok(text) => text,
            Err(_) => String::new(),
        }
    }

    // ── Mise à jour de l'état ───────────────────────────────────────────

    /// Met à jour la tâche courante.
    pub async fn set_current_task(&self, task: &str) {
        self.state.write().await.current_task = task.to_string();
    }

    /// Ajoute une décision importante.
    pub async fn add_decision(&self, decision: &str) {
        let mut state = self.state.write().await;
        state.recent_decisions.push(decision.to_string());
        if state.recent_decisions.len() > 20 {
            state.recent_decisions.remove(0);
        }
    }

    /// Ajoute une erreur/correction.
    pub async fn add_error(&self, error: &str) {
        let mut state = self.state.write().await;
        state.recent_errors.push(error.to_string());
        if state.recent_errors.len() > 10 {
            state.recent_errors.remove(0);
        }
    }

    /// Définit l'objectif de session.
    pub async fn set_session_goal(&self, goal: &str) {
        self.state.write().await.session_goal = goal.to_string();
    }

    /// Ajoute une prochaine étape.
    pub async fn add_next_step(&self, step: &str) {
        let mut state = self.state.write().await;
        state.next_steps.push(step.to_string());
        if state.next_steps.len() > 10 {
            state.next_steps.remove(0);
        }
    }

    /// Incrémente le compteur de messages.
    pub async fn increment_message_count(&self) -> u64 {
        let mut state = self.state.write().await;
        state.message_count += 1;
        state.message_count
    }

    // ── Détection de compaction ─────────────────────────────────────────

    /// Vérifie si un signal de compaction est présent (trigger file).
    pub async fn check_compaction_trigger(&self) -> bool {
        match tokio::fs::read_to_string(&self.trigger_path).await {
            Ok(content) if content.trim() == "1" => {
                // Effacer le trigger
                let _ = tokio::fs::write(&self.trigger_path, "0").await;
                true
            }
            _ => false,
        }
    }

    /// Nettoie les fichiers d'état (appelé après restauration réussie).
    pub async fn clear_state(&self) -> anyhow::Result<()> {
        let _ = tokio::fs::remove_file(&self.state_path).await;
        let _ = tokio::fs::remove_file(&self.restore_path).await;
        // Réinitialiser l'état en mémoire
        *self.state.write().await = ContextState::new();
        info!("CompactionWatchdog: état effacé");
        Ok(())
    }

    /// Boucle de surveillance principale.
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
            self.config.check_interval_secs,
        ));
        loop {
            interval.tick().await;

            // Vérifier le trigger de compaction
            if self.check_compaction_trigger().await {
                warn!("CompactionWatchdog: signal de compaction détecté !");
                if let Err(e) = self.on_compaction().await {
                    tracing::error!("CompactionWatchdog: erreur lors de la sauvegarde: {}", e);
                }
            }

            // Sauvegarde proactive si le seuil de messages est atteint
            if self.config.proactive_save {
                let msg_count = self.state.read().await.message_count;
                if msg_count > 0 && msg_count % self.config.message_threshold == 0 {
                    if let Err(e) = self.save_state().await {
                        tracing::error!("CompactionWatchdog: erreur sauvegarde proactive: {}", e);
                    }
                }
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
    async fn test_save_and_restore_state() {
        let dir = TempDir::new().unwrap();
        let cfg = CompactionConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let wd = CompactionWatchdog::new(cfg);

        wd.set_session_goal("Tester la sauvegarde").await;
        wd.set_current_task("Écrire des tests").await;
        wd.add_decision("Utiliser TempDir pour les tests").await;
        wd.add_error("Erreur simulée - corrigée").await;
        wd.add_next_step("Vérifier la restauration").await;

        wd.save_state().await.unwrap();

        // Vérifier que le fichier existe
        assert!(wd.state_path.exists());

        // Restaurer dans un nouveau watchdog
        let wd2 = CompactionWatchdog::new(CompactionConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        });
        let restored = wd2.load_previous_state().await.unwrap();
        assert!(restored.is_some());
        let s = restored.unwrap();
        assert_eq!(s.session_goal, "Tester la sauvegarde");
        assert_eq!(s.current_task, "Écrire des tests");
        assert_eq!(s.recent_decisions.len(), 1);
        assert_eq!(s.recent_errors.len(), 1);
        assert_eq!(s.next_steps.len(), 1);
    }

    #[tokio::test]
    async fn test_compaction_trigger() {
        let dir = TempDir::new().unwrap();
        let cfg = CompactionConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let wd = CompactionWatchdog::new(cfg);

        // Écrire le trigger
        tokio::fs::write(&wd.trigger_path, "1").await.unwrap();
        assert!(wd.check_compaction_trigger().await);

        // Le trigger doit être effacé
        let content = tokio::fs::read_to_string(&wd.trigger_path).await.unwrap();
        assert_eq!(content.trim(), "0");
    }

    #[tokio::test]
    async fn test_format_for_context() {
        let mut state = ContextState::new();
        state.session_goal = "Tester".to_string();
        state.current_task = "Écriture".to_string();
        state.recent_decisions.push("Décision #1".to_string());
        state.recent_errors.push("Erreur #1".to_string());
        state.next_steps.push("Étape #1".to_string());

        let text = state.format_for_context();
        assert!(text.contains("Objectif"));
        assert!(text.contains("Tâche en cours"));
        assert!(text.contains("Décisions récentes"));
        assert!(text.contains("Erreurs corrigées"));
        assert!(text.contains("Prochaines étapes"));
        assert!(text.contains("Reprenez votre travail"));
    }
}
