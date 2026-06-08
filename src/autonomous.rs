//! # AutonomousEntity — Assemblage des 4 crates autonomes
//!
//! L'entité qui lie soul_llm, soul_planner, soul_tools et soul_repl.
//!
//! ## Exemple
//! ```ignore
//! use soulsystem_bin::autonomous::AutonomousEntity;
//! use soul_llm::LlmConfig;
//! let config = LlmConfig::default();
//! let mut entity = AutonomousEntity::new(config, "mon-server");
//! entity.is_alive().await;
//! let goal = entity.create_goal("Sauvegarder la base de données");
//! let plan = entity.plan(&goal);
//! let reponse = entity.ask("Quel est l'état du système?").await?;
//! ```

use soul_llm::{LlmConfig, OllamaClient};
use soul_planner::{CognitiveLoop, Goal, GoalStatus};
use soul_tools::ToolRegistry;
use uuid::Uuid;

pub struct AutonomousEntity {
    pub llm: OllamaClient,
    pub planner: CognitiveLoop,
    pub tools: ToolRegistry,
    pub entity_name: String,
}

impl AutonomousEntity {
    /// Créer une nouvelle entité autonome
    pub fn new(config: LlmConfig, name: &str) -> Self {
        let mut reg = ToolRegistry::new();
        for t in soul_tools::discover_system_tools() {
            reg.register(t);
        }
        Self {
            llm: OllamaClient::new(config),
            planner: CognitiveLoop::new(),
            tools: reg,
            entity_name: name.to_string(),
        }
    }

    /// Vérifier la connectivité Ollama
    pub async fn is_alive(&self) -> bool {
        self.llm.is_alive().await
    }

    /// Créer un objectif
    pub fn create_goal(&self, description: &str) -> Goal {
        Goal {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority: 5,
            created_at: chrono::Utc::now(),
            status: GoalStatus::Active,
        }
    }

    /// Créer un plan pour un objectif
    pub fn plan(&self, goal: &Goal) -> soul_planner::Plan {
        self.planner.create_plan(
            goal,
            &[
                "observe",   // 1. Observe
                "analyze",   // 2. Analyse
                "execute",   // 3. Exécute
                "evaluate",  // 4. Évalue
            ],
        )
    }

    /// Poser une question au LLM
    pub async fn ask(&self, message: &str) -> Result<String, String> {
        let resp = self.llm.generate(message).await.map_err(|e| e.to_string())?;
        Ok(resp.response)
    }

    /// Exécuter le plan (simplifié : exécute chaque étape comme commande shell)
    pub fn execute_plan(&self, plan: &soul_planner::Plan) -> Result<String, String> {
        let mut results = Vec::new();
        for step in &plan.steps {
            match soul_tools::execute_shell(&step.command) {
                Ok(output) => {
                    self.planner.history.record(
                        step.command.clone(),
                        output.clone(),
                        true,
                    );
                    results.push(format!("[OK] {}: {}", step.command, output.lines().next().unwrap_or("")));
                }
                Err(e) => {
                    self.planner.history.record(
                        step.command.clone(),
                        e.clone(),
                        false,
                    );
                    results.push(format!("[ERR] {}: {}", step.command, e));
                }
            }
        }
        Ok(results.join("\n"))
    }

    /// État du système (JSON)
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "entity": self.entity_name,
            "tools_count": self.tools.all().len(),
            "success_rate": self.planner.history.success_rate(),
            "memory_size": self.planner.memory.recent_observations(10).len(),
            "goals": {
                "active": 0,
                "completed": 0,
                "failed": 0,
            }
        })
    }
}
