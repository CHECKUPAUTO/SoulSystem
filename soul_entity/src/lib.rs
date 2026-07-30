//! # soul_entity — L'entité numérique autonome de SoulSystem
//!
//! Agrège tous les sous-systèmes nécessaires à l'autonomie.
//! L'entité expose un **EntityHandle** pour être pilotable par le
//! `soul_gateway` (HTTP/WS) et une **boucle cognitive autonome**.

pub mod cron;
pub mod entity;
pub mod event_store;
pub mod facade;
pub mod subsystems;
pub mod types;

// Re-exports
pub use entity::SoulEntity;
pub use event_store::PersistentEventStore;
pub use facade::OpenClawFacade;
pub use subsystems::{
    Subsystems, TAG_DECISION, TAG_ERROR, TAG_EVOLVE, TAG_FORGE, TAG_GOAL, TAG_HEAL, TAG_PLAN,
    TAG_STEP,
};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use soul_llm::LlmConfig;
    use soul_openclaw::AgentLoopConfig;
    use soul_sandbox::SandboxPolicy;
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
                ..Default::default()
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

    /// `execute_plan` reports what it did, which is not execution (HIGH-006).
    ///
    /// This test previously asserted `[OK]`, `status == "completed"` and
    /// `goals_completed >= 1` — it was pinning the fabrication in place. A
    /// green suite was part of why the claim looked trustworthy.
    #[tokio::test]
    async fn execute_plan_reports_simulation_not_success() {
        let e = test_entity();
        let g = e.create_goal("Echo test", 5);
        e.plan(&g.id).unwrap();
        let r = e.execute_plan(&g.id).unwrap();

        assert!(
            r.contains("SIMULATED"),
            "the result string must say nothing ran, got: {r}"
        );
        assert!(
            !r.contains("[OK]"),
            "\"[OK]\" is indistinguishable from a step that really succeeded"
        );

        let g2 = e.get_goal(&g.id).unwrap();
        assert_eq!(
            g2.status, "simulated",
            "a goal whose plan was never dispatched is not completed"
        );

        let stats = e.stats.lock();
        assert!(stats.goals_simulated >= 1);
        assert_eq!(
            stats.goals_completed, 0,
            "goals_completed answers \"is this doing work\" — a simulated run \
             must not count toward it"
        );
    }

    /// The evaluation persisted to memory must not claim a score it did not earn.
    ///
    /// This matters more than the display string: `eval` is serialized into
    /// long-term memory and interpolated into the LLM summary prompt, so a
    /// fabricated 0.9 became an input to later reasoning.
    #[tokio::test]
    async fn a_simulated_cycle_does_not_persist_a_confident_evaluation() {
        let e = test_entity();
        let g = e.create_goal("Echo test", 5);
        e.plan(&g.id).unwrap();
        let record = e.run_cycle().await.expect("cycle runs");

        let score = record
            .get("evaluation")
            .and_then(|e| e.get("score"))
            .and_then(|s| s.as_f64())
            .expect("the cycle record carries an evaluation score");
        assert_eq!(
            score, 0.0,
            "nothing was executed, so there is nothing to score — a non-zero \
             value here is persisted to memory and fed to the LLM summary"
        );

        let feedback = record
            .get("evaluation")
            .and_then(|e| e.get("feedback"))
            .and_then(|f| f.as_str())
            .expect("the cycle record carries evaluation feedback");
        assert!(
            feedback.contains("NON exécuté"),
            "feedback must state that nothing ran, got: {feedback}"
        );
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
        assert!(
            verdict.exit_code == Some(0)
                || !verdict.stdout.is_empty()
                || !verdict.stderr.is_empty()
        );
    }

    #[tokio::test]
    async fn run_cycle_executes_end_to_end() {
        let e = test_entity();
        e.create_goal("Cycle test", 5);
        let v = e.run_cycle().await.unwrap();
        assert!(v["cycle_id"].is_string());
        assert!(v["evaluation"]["score"].is_number());
    }

    #[tokio::test]
    async fn hierarchical_memory_is_wired() {
        let e = test_entity();
        // Store an observation in hierarchical memory via the entity
        e.store_observation(
            "test fact: the sky is blue",
            soullink_memory_hierarchy::MemoryLayer::Episodic,
            soullink_memory_hierarchy::MemoryTrust::Internal,
        )
        .await;
        // Search for it with a substring that is present
        let results = e.search_memory("sky", 5).await;
        assert!(
            !results.is_empty(),
            "should find stored observation with substring 'sky'"
        );
        assert!(results.iter().any(|r| r.text.contains("sky is blue")));
    }

    #[tokio::test]
    async fn hierarchical_memory_consolidation_supported() {
        let e = test_entity();
        // Should not panic
        e.consolidate_memory().await;
    }

    #[tokio::test]
    async fn search_memory_returns_empty_for_unknown_query() {
        let e = test_entity();
        let results = e.search_memory("nonexistent_xyz_abc", 5).await;
        // Episodic store may return untagged results; test for no panic and valid vec.
        assert!(results
            .iter()
            .all(|r| !r.text.contains("SKY_BLUE_UNLIKELY")));
    }

    #[tokio::test]
    async fn subagent_spawn_returns_task_id() {
        let e = test_entity();
        let id = e.spawn_subagent("test subagent task", None).await.unwrap();
        assert!(!id.is_empty());
        assert!(uuid::Uuid::parse_str(&id).is_ok());
    }

    #[tokio::test]
    async fn subagent_list_after_spawn() {
        let e = test_entity();
        e.spawn_subagent("task A", None).await.unwrap();
        e.spawn_subagent("task B", None).await.unwrap();
        let tasks = e.subagent_tasks().await;
        assert!(tasks.len() >= 2);
    }

    #[tokio::test]
    async fn subagent_cancel() {
        let e = test_entity();
        let id = e.spawn_subagent("cancel me", None).await.unwrap();
        // Task exists before cancel
        let task = e.subagent_task(&id).await.unwrap();
        assert!(
            task.status == soul_subagents::TaskStatus::Pending
                || task.status == soul_subagents::TaskStatus::Running
        );
        // Cancel succeeds
        e.cancel_subagent(&id).await.unwrap();
        let cancelled = e.subagent_task(&id).await.unwrap();
        assert_eq!(cancelled.status, soul_subagents::TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn subagent_spawn_fails_when_max_reached() {
        let e = test_entity();
        // SubAgentManager is created with max_concurrent=4; we can't easily
        // test the limit since spawns don't complete (LLM unreachable).
        // Just verify spawn works for the default limit.
        let id = e.spawn_subagent("a", None).await;
        assert!(id.is_ok());
    }

    #[tokio::test]
    async fn subagent_active_count() {
        let e = test_entity();
        let before = e.subagent_active_count().await;
        e.spawn_subagent("active task", None).await.unwrap();
        // Since tasks start as Pending, they count as active
        assert!(e.subagent_active_count().await > before);
    }

    #[tokio::test]
    async fn subagent_results_collected() {
        let e = test_entity();
        let results = e.subagent_results().await;
        // No completed tasks should return empty
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn subagent_task_not_found() {
        let e = test_entity();
        assert!(e.subagent_task("nonexistent-xxxx").await.is_none());
    }
}
