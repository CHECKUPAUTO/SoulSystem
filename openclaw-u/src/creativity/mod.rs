//! Creativity — Génération de nouveaux types d'actions

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// Moteur de créativité pour inventer de nouvelles actions
#[derive(Debug, Clone)]
pub struct CreativityEngine {
    /// Actions de base connues
    pub base_actions: Vec<String>,
    /// Actions inventées avec leur score
    pub invented: HashMap<String, CreativeAction>,
    /// Historique des inventions
    pub invention_history: Vec<InventionRecord>,
}

#[derive(Debug, Clone)]
pub struct CreativeAction {
    pub name: String,
    pub description: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub success_count: u32,
    pub failure_count: u32,
    pub avg_reward: f64,
    pub born_cycle: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventionRecord {
    pub cycle: u64,
    pub action_name: String,
    pub method: InventionMethod,
    pub result: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InventionMethod {
    /// Combinaison de deux actions existantes
    Combine(String, String),
    /// Variation d'une action existante
    Variate(String),
    /// Inspiration d'un pattern système
    PatternInspired(String),
    /// Mutation aléatoire guidée
    GuidedMutation,
}

impl Default for CreativityEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CreativityEngine {
    pub fn new() -> Self {
        let base = vec![
            "optimize_system".to_string(),
            "restart_service".to_string(),
            "investigate".to_string(),
            "report".to_string(),
            "wait".to_string(),
            "explore".to_string(),
            "alert".to_string(),
            "checkpoint_state".to_string(),
            "health_check".to_string(),
            "memory_analysis".to_string(),
        ];

        Self {
            base_actions: base.clone(),
            invented: HashMap::new(),
            invention_history: Vec::new(),
        }
    }

    /// Invente une nouvelle action
    pub fn invent(
        &mut self,
        cycle: u64,
        _system_state: &crate::perception::SystemSnapshot,
    ) -> Option<CreativeAction> {
        let methods = [
            InventionMethod::Combine(self.base_actions[0].clone(), self.base_actions[1].clone()),
            InventionMethod::Variate(self.base_actions[2].clone()),
            InventionMethod::PatternInspired("high_cpu".to_string()),
            InventionMethod::GuidedMutation,
        ];

        let method = methods.choose(&mut rand::thread_rng())?;

        let action = match method {
            InventionMethod::Combine(a, b) => {
                let name = format!("{}_and_{}", a, b);
                CreativeAction {
                    name: name.clone(),
                    description: format!("Combine {} et {} pour optimisation systémique", a, b),
                    inputs: vec!["system_state".to_string()],
                    outputs: vec!["optimized_state".to_string()],
                    success_count: 0,
                    failure_count: 0,
                    avg_reward: 0.0,
                    born_cycle: cycle,
                }
            }
            InventionMethod::Variate(base) => {
                let name = format!("deep_{}", base);
                CreativeAction {
                    name: name.clone(),
                    description: format!("Version approfondie de {}", base),
                    inputs: vec!["system_state".to_string(), "history".to_string()],
                    outputs: vec!["detailed_analysis".to_string()],
                    success_count: 0,
                    failure_count: 0,
                    avg_reward: 0.0,
                    born_cycle: cycle,
                }
            }
            InventionMethod::PatternInspired(pattern) => {
                let name = format!("react_to_{}", pattern);
                CreativeAction {
                    name: name.clone(),
                    description: format!("Réaction proactive au pattern {}", pattern),
                    inputs: vec![pattern.clone()],
                    outputs: vec!["mitigation".to_string()],
                    success_count: 0,
                    failure_count: 0,
                    avg_reward: 0.0,
                    born_cycle: cycle,
                }
            }
            InventionMethod::GuidedMutation => {
                let name = format!("novel_action_{}", cycle);
                CreativeAction {
                    name: name.clone(),
                    description: "Action nouvellement inventée par mutation guidée".to_string(),
                    inputs: vec!["sensor_data".to_string()],
                    outputs: vec!["action_result".to_string()],
                    success_count: 0,
                    failure_count: 0,
                    avg_reward: 0.0,
                    born_cycle: cycle,
                }
            }
        };

        info!(
            "🎨 CRÉATIVITÉ: invention '{}' via {:?}",
            action.name, method
        );

        self.invention_history.push(InventionRecord {
            cycle,
            action_name: action.name.clone(),
            method: method.clone(),
            result: true,
        });

        self.invented.insert(action.name.clone(), action.clone());
        Some(action)
    }

    /// Teste une action inventée
    pub async fn test_action(
        &mut self,
        name: &str,
        _llm: &crate::llm::LlmEngine,
    ) -> Result<bool, String> {
        let action = self
            .invented
            .get(name)
            .ok_or_else(|| "Action inconnue".to_string())?;

        info!("🎨 CRÉATIVITÉ: test de '{}'", name);

        // Simuler un test basique
        let _test_prompt = format!(
            "Test de l'action '{}' — {}. Description: {}. \
            Cette action est-elle cohérente avec le système ? Réponds par oui/non.",
            action.name, action.description, action.description
        );

        // Pour l'instant, simuler le résultat
        // NOTE: utiliser le LLM pour valider
        let simulated_success = action.name.len() < 50 && !action.inputs.is_empty();

        if simulated_success {
            info!("✅ CRÉATIVITÉ: '{}' validée", name);
            if let Some(a) = self.invented.get_mut(name) {
                a.success_count += 1;
                a.avg_reward = 0.5;
            }
        } else {
            warn!("❌ CRÉATIVITÉ: '{}' rejetée", name);
            if let Some(a) = self.invented.get_mut(name) {
                a.failure_count += 1;
                a.avg_reward = -0.3;
            }
        }

        Ok(simulated_success)
    }

    /// Récupère les actions inventées qui ont réussi
    pub fn get_successful_actions(&self, min_success: u32) -> Vec<&CreativeAction> {
        self.invented
            .values()
            .filter(|a| a.success_count >= min_success)
            .collect()
    }

    /// Fusionne les actions inventées dans le LLM
    pub fn augment_llm_actions(&self) -> Vec<String> {
        let mut actions = self.base_actions.clone();
        for name in self.invented.keys() {
            if !actions.contains(name) {
                actions.push(name.clone());
            }
        }
        actions
    }

    /// Génère une suggestion basée sur la créativité
    pub fn suggest_novel_action(&self, energy: f64) -> Option<String> {
        if energy > 7.0 && !self.invented.is_empty() {
            // Quand l'énergie est haute, explorer
            let novel = self.invented.values().max_by_key(|a| a.success_count)?;
            Some(novel.name.clone())
        } else if energy > 4.0 {
            // Énergie moyenne: combinaison
            Some("optimize_and_investigate".to_string())
        } else {
            None
        }
    }

    /// Sauvegarde l'historique des inventions
    pub fn save(&self) {
        let path = "/tmp/openclaw_u_creativity.json";
        if let Ok(json) = serde_json::to_string_pretty(&self.invention_history) {
            let _ = std::fs::write(path, json);
        }
    }
}
