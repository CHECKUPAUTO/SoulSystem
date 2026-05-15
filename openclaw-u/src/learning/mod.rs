//! Learning — Apprentissage par renforcement basé sur les résultats passés

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Score historique d'une action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionScore {
    pub action: String,
    pub total_reward: f64,
    pub count: u64,
    pub avg_reward: f64,
    pub last_seen: String,
    pub success_rate: f32,
}

/// Table de Q-learning simplifiée
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QTable {
    pub scores: HashMap<String, ActionScore>,
    pub learning_rate: f64,
    pub discount: f64,
}

impl Default for QTable {
    fn default() -> Self {
        Self::new()
    }
}

impl QTable {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
            learning_rate: 0.3,
            discount: 0.9,
        }
    }

    /// Met à jour le score d'une action
    pub fn update(&mut self, action: &str, reward: f64, success: bool) {
        let entry = self
            .scores
            .entry(action.to_string())
            .or_insert(ActionScore {
                action: action.to_string(),
                total_reward: 0.0,
                count: 0,
                avg_reward: 0.0,
                last_seen: chrono::Utc::now().to_rfc3339(),
                success_rate: 0.0,
            });

        entry.count += 1;
        entry.total_reward += reward;
        entry.avg_reward = entry.total_reward / entry.count as f64;
        entry.last_seen = chrono::Utc::now().to_rfc3339();

        // Mise à jour du taux de succès
        let success_val = if success { 1.0 } else { 0.0 };
        if entry.count == 1 {
            entry.success_rate = success_val;
        } else {
            entry.success_rate = (entry.success_rate * ((entry.count - 1) as f32) + success_val)
                / entry.count as f32;
        }
    }

    /// Score combiné (récompense moyenne × taux de succès)
    pub fn score(&self, action: &str) -> f32 {
        match self.scores.get(action) {
            Some(s) => (s.avg_reward as f32 * s.success_rate).max(0.0),
            None => 0.5, // Score neutre pour nouvelles actions
        }
    }

    /// Meilleure action connue
    pub fn best_action(&self) -> Option<&str> {
        self.scores
            .iter()
            .max_by(|(_, a), (_, b)| {
                a.avg_reward
                    .partial_cmp(&b.avg_reward)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, _)| k.as_str())
    }

    /// Privilégier les actions réussies dans le prompt LLM
    pub fn to_context(&self) -> String {
        let mut lines = vec!["Historique des actions:".to_string()];
        let mut sorted: Vec<&ActionScore> = self.scores.values().collect();
        sorted.sort_by(|a, b| {
            b.avg_reward
                .partial_cmp(&a.avg_reward)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for s in sorted.iter().take(5) {
            lines.push(format!(
                "  {}: avg={:.2} | success={:.0}% | n={}",
                s.action,
                s.avg_reward,
                s.success_rate * 100.0,
                s.count
            ));
        }
        lines.join("\n")
    }

    /// Sauvegarde
    pub fn save(&self) {
        let path = "/tmp/openclaw_u_qtable.json";
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Chargement
    pub fn load() -> Self {
        let path = std::path::Path::new("/tmp/openclaw_u_qtable.json");
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(table) = serde_json::from_str::<Self>(&data) {
                return table;
            }
        }
        Self::new()
    }
}
