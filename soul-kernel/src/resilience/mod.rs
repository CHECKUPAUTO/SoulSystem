//! Resilience — Auto-réparation par détection d'échecs et changement de stratégie

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// Historique des échecs par action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureLog {
    pub action: String,
    pub count: u32,
    pub last_error: String,
    pub first_seen: String,
    pub last_seen: String,
}

/// Moteur de résilience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceEngine {
    pub failures: HashMap<String, FailureLog>,
    pub max_retries: u32,
    pub cooldown_cycles: u64,
    pub last_action: String,
    pub consecutive_same_action: u32,
}

impl ResilienceEngine {
    pub fn new() -> Self {
        Self {
            failures: HashMap::new(),
            max_retries: 3,
            cooldown_cycles: 5,
            last_action: String::new(),
            consecutive_same_action: 0,
        }
    }

    /// Enregistre un échec
    pub fn record_failure(&mut self, action: &str, error: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let entry = self
            .failures
            .entry(action.to_string())
            .or_insert(FailureLog {
                action: action.to_string(),
                count: 0,
                last_error: String::new(),
                first_seen: now.clone(),
                last_seen: now.clone(),
            });
        entry.count += 1;
        entry.last_error = error.to_string();
        entry.last_seen = now;

        warn!(
            "⚠️  RESILIENCE: {} échecs pour '{}' | dernière: {}",
            entry.count, action, error
        );
    }

    /// Enregistre un succès (réinitialise le compteur)
    pub fn record_success(&mut self, action: &str) {
        if let Some(entry) = self.failures.get_mut(action) {
            if entry.count > 0 {
                info!(
                    "✅ RESILIENCE: '{}' réussi après {} échecs",
                    action, entry.count
                );
                entry.count = 0;
            }
        }
    }

    /// Détecte une boucle d'échec
    pub fn is_failing(&self, action: &str) -> bool {
        self.failures
            .get(action)
            .map(|f| f.count >= self.max_retries)
            .unwrap_or(false)
    }

    /// Détecte une boucle d'action identique
    pub fn is_looping(&self, action: &str) -> bool {
        self.last_action == action && self.consecutive_same_action >= 5
    }

    /// Suggère une stratégie alternative
    pub fn suggest_fallback(&self, action: &str) -> Option<String> {
        if self.is_failing(action) {
            Some(match action {
                "investigate" => "report".to_string(),
                "optimize_system" => "wait".to_string(),
                "restart_service" => "alert".to_string(),
                "explore" => "wait".to_string(),
                "auto_evolve" => "checkpoint_state".to_string(),
                _ => "wait".to_string(),
            })
        } else {
            None
        }
    }

    /// Ajuste le heartbeat selon la santé
    pub fn suggest_heartbeat(&self, _cycle: u64) -> u64 {
        let total_failures: u32 = self.failures.values().map(|f| f.count).sum();
        if total_failures > 10 {
            60 // Ralentir si beaucoup d'échecs
        } else if total_failures > 5 {
            45
        } else {
            30
        }
    }

    /// Suggère un modèle LLM alternatif
    pub fn suggest_model(&self, current: &str) -> Option<String> {
        let total_failures: u32 = self.failures.values().map(|f| f.count).sum();
        if total_failures > 5 && current.contains("1.5b") {
            Some("qwen2.5-coder:7b".to_string())
        } else if total_failures > 10 && !current.contains("deepseek") {
            Some("deepseek-coder-v2:16b".to_string())
        } else {
            None
        }
    }

    /// Met à jour l'action courante
    pub fn set_last_action(&mut self, action: &str) {
        if self.last_action == action {
            self.consecutive_same_action += 1;
        } else {
            self.consecutive_same_action = 1;
            self.last_action = action.to_string();
        }
    }

    /// Rapport de santé
    pub fn health_report(&self) -> String {
        let total: u32 = self.failures.values().map(|f| f.count).sum();
        let failing: Vec<&str> = self
            .failures
            .iter()
            .filter(|(_, f)| f.count >= self.max_retries)
            .map(|(k, _)| k.as_str())
            .collect();

        format!(
            "Resilience: {} échecs totaux | actions bloquées: {:?} | loop: {}",
            total, failing, self.consecutive_same_action
        )
    }

    pub fn save(&self) {
        let path = "/tmp/soul_kernel_resilience.json";
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load() -> Self {
        let path = std::path::Path::new("/tmp/soul_kernel_resilience.json");
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(r) = serde_json::from_str::<Self>(&data) {
                return r;
            }
        }
        Self::new()
    }
}

impl Default for ResilienceEngine {
    fn default() -> Self {
        Self::new()
    }
}
