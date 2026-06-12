//! # soul_entity — L'entité numérique autonome de SoulSystem
//!
//! Agrège tous les sous-systèmes nécessaires à l'autonomie.
//! L'entité expose un **EntityHandle** pour être pilotable par le
//! `soul_gateway` (HTTP/WS) et une **boucle cognitive autonome**.

pub mod types;
pub mod event_store;
pub mod facade;
pub mod entity;
pub mod subsystems;

// Re-exports
pub use entity::SoulEntity;
pub use event_store::PersistentEventStore;
pub use facade::OpenClawFacade;
pub use types::*;
pub use subsystems::{Subsystems, TAG_DECISION, TAG_ERROR, TAG_EVOLVE, TAG_FORGE, TAG_GOAL, TAG_HEAL, TAG_PLAN, TAG_STEP};

#[cfg(test)]
mod tests {
    use super::*;
    use soul_llm::LlmConfig;
    use soul_sandbox::SandboxPolicy;
    use soul_openclaw::AgentLoopConfig;
    use std::time::Duration;

    fn test_entity() -> SoulEntity {
        let cfg = EntityConfig {
            name: "test".into(),
            llm: LlmConfig {
                provider: soul_llm::ProviderKind::Ollama,
                base_url: "http://127.0.0.1:1".into(),
                model: "test".into(),
                temperature: 0.0,
                http_timeout: Duration::from_secs(30),
                connect_timeout: Duration::from_secs(5),
                auth_token: None,
                max_tokens: 8,
                goal_token_budget: 0,
                tokens_per_minute_budget: 0,
                pool_max_idle: 1,
                pool_idle_timeout: Duration::from_secs(1),
            },
            sandbox_policy: SandboxPolicy::default(),
            loop_config: AgentLoopConfig::default(),
            autonomous_tick: Duration::from_millis(50),
            memory_path: None,
            event_store_path: None,
            max_goal_history: 10,
        };
        SoulEntity::new(cfg).unwrap()
    }

    #[tokio::test]
    async fn entity_constructs() {
        let e = test_entity();
        assert_eq!(e.config.name, "test");
        assert!(!e.is_running());
    }

    #[tokio::test]
    async fn create_goal_persists_in_memory() {
        let e = test_entity();
        let g = e.create_goal("Test goal", 5);
        assert_eq!(e.list_goals().len(), 1);
        assert!(e.memory.get(&g.id).is_ok());
    }

    #[tokio::test]
    async fn plan_attaches_steps() {
        let e = test_entity();
        let g = e.create_goal("Faire X", 5);
        let p = e.plan(&g.id).unwrap();
        assert_eq!(p.steps.len(), 4);
        let g2 = e.get_goal(&g.id).unwrap();
        assert!(g2.plan.is_some());
        assert_eq!(g2.status, "planned");
    }

    #[tokio::test]
    async fn execute_plan_records_results() {
        let e = test_entity();
        let g = e.create_goal("Echo test", 5);
        e.plan(&g.id).unwrap();
        let r = e.execute_plan(&g.id).unwrap();
        assert!(r.contains("[OK]"));
        let g2 = e.get_goal(&g.id).unwrap();
        assert_eq!(g2.status, "completed");
        assert!(e.stats.lock().goals_completed >= 1);
    }

    #[tokio::test]
    async fn sandbox_blocks_dangerous() {
        let e = test_entity();
        let r = e.execute_shell("rm -rf /");
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn status_returns_json() {
        let e = test_entity();
        let s = e.status();
        assert_eq!(s["entity"], "test");
        assert!(s["goals_total"].as_u64().is_some());
    }

    #[tokio::test]
    async fn code_artifact_generation() {
        let e = test_entity();
        let src = "print('hi from soul')";
        let (artifact, verdict) = e.generate_and_run("python", src).unwrap();
        assert_eq!(artifact.language, "python");
        assert!(verdict.exit_code == Some(0) || !verdict.stdout.is_empty() || !verdict.stderr.is_empty());
    }

    #[tokio::test]
    async fn run_cycle_executes_end_to_end() {
        let e = test_entity();
        e.create_goal("Cycle test", 5);
        let v = e.run_cycle().await.unwrap();
        assert!(v["cycle_id"].is_string());
        assert!(v["evaluation"]["score"].is_number());
    }
}
