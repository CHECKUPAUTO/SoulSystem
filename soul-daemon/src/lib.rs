//! Background autonomous daemon with persistent goals and signal handling.
//!
//! Runs a continuous loop that:
//! 1. Loads persistent goals from sled
//! 2. Picks highest-priority active goal
//! 3. Executes via soul_agent_core ReAct loop
//! 4. Handles SIGINT/SIGTERM for graceful shutdown
//! 5. Logs all actions to persistent store

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use soul_agent_core::{AgentEvent, AutonomousAgent};
use soul_eventbus::{AgentEvent as BusEvent, EventBus};
use soul_llm::{LlmConfig, OllamaClient};
use soul_memory::{GoalStatus, PersistentGoal, PersistentStore};
use soul_subagents::SubAgentManager;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

#[derive(Error, Debug)]
pub enum DaemonError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Persist error: {0}")]
    Persist(#[from] soul_memory::PersistError),
    #[error("Agent error: {0}")]
    Agent(String),
    #[error("LLM error: {0}")]
    Llm(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub poll_interval_secs: u64,
    pub max_turns_per_goal: usize,
    pub max_concurrent_goals: usize,
    pub auto_retry_failed: bool,
    pub model: String,
    pub ollama_url: String,
    /// Where the memory-provenance index lives (INV-MEM-3 / MED-015-B).
    ///
    /// `Some` makes every per-goal agent load the index before running and
    /// persist it after, so provenance survives both goal turnover and daemon
    /// restarts. `None` keeps the old behaviour: each goal starts blind and
    /// its provenance dies with it.
    ///
    /// Caveat, stated rather than hidden: with `max_concurrent_goals > 1`,
    /// concurrent goals each load-modify-persist this one file. The write is
    /// atomic (temp + rename), so the index can never be *corrupt*, but the
    /// last persist wins and records from an overlapping goal can be lost —
    /// they come back `Unknown`, not wrong. Merging is follow-up work.
    pub provenance_path: Option<std::path::PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 5,
            max_turns_per_goal: 20,
            max_concurrent_goals: 1,
            auto_retry_failed: false,
            model: "qwen3:8b".into(),
            ollama_url: "http://localhost:11434".into(),
            provenance_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub started_at: DateTime<Utc>,
    pub goals_completed: u64,
    pub goals_failed: u64,
    pub goals_active: u64,
    pub is_running: bool,
    pub last_goal_id: Option<String>,
    pub total_turns: u64,
}

pub struct Daemon {
    config: DaemonConfig,
    store: Arc<PersistentStore>,
    llm_config: LlmConfig,
    sub_agents: Arc<SubAgentManager>,
    event_bus: EventBus,
    shutdown_tx: watch::Sender<bool>,
    goal_tracker: Arc<tokio::sync::RwLock<GoalTracker>>,
}

/// Tracks running goals for stall detection and watchdog
#[derive(Debug, Default)]
struct GoalTracker {
    /// goal_id → (started_at, turn_count, last_activity)
    running: HashMap<String, (DateTime<Utc>, u64, DateTime<Utc>)>,
    /// goal_id → stall_count (how many times it was flagged)
    stall_counts: HashMap<String, u32>,
}

impl GoalTracker {
    fn start_goal(&mut self, goal_id: &str) {
        let now = Utc::now();
        self.running.insert(goal_id.to_string(), (now, 0, now));
        self.stall_counts.insert(goal_id.to_string(), 0);
    }

    fn update_activity(&mut self, goal_id: &str, turn: u64) {
        if let Some(entry) = self.running.get_mut(goal_id) {
            entry.1 = turn;
            entry.2 = Utc::now();
        }
    }

    fn finish_goal(&mut self, goal_id: &str) {
        self.running.remove(goal_id);
        self.stall_counts.remove(goal_id);
    }

    /// Check if a goal is stalled (no activity for stall_timeout_secs)
    fn check_stalled(&self, goal_id: &str, stall_timeout_secs: u64) -> Option<u32> {
        if let Some((_, _, last_activity)) = self.running.get(goal_id) {
            let elapsed = Utc::now()
                .signed_duration_since(*last_activity)
                .num_seconds() as u64;
            if elapsed > stall_timeout_secs {
                let count = self.stall_counts.get(goal_id).copied().unwrap_or(0) + 1;
                return Some(count);
            }
        }
        None
    }

    fn is_running(&self, goal_id: &str) -> bool {
        self.running.contains_key(goal_id)
    }

    /// Number of in-flight goals — exposed for monitoring/backpressure.
    #[allow(dead_code)]
    fn running_count(&self) -> usize {
        self.running.len()
    }
}

impl Daemon {
    pub fn new(config: DaemonConfig, store: PersistentStore) -> Self {
        let llm_config = LlmConfig {
            base_url: config.ollama_url.clone(),
            model: config.model.clone(),
            ..Default::default()
        };
        let max_concurrent = config.max_concurrent_goals;
        let (shutdown_tx, _) = watch::channel(false);

        Self {
            config,
            store: Arc::new(store),
            llm_config: llm_config.clone(),
            sub_agents: Arc::new(SubAgentManager::new(llm_config, max_concurrent)),
            event_bus: EventBus::new(1024),
            shutdown_tx,
            goal_tracker: Arc::new(tokio::sync::RwLock::new(GoalTracker::default())),
        }
    }

    /// Get the event bus handle for external subscribers
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Add a goal to the daemon
    pub fn add_goal(&self, description: &str, priority: u8) -> Result<PersistentGoal, DaemonError> {
        let goal = PersistentGoal {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority,
            status: GoalStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parent_id: None,
            children_ids: vec![],
            result: None,
        };
        self.store.save_goal(&goal)?;
        Ok(goal)
    }

    /// Get current daemon state
    pub fn state(&self) -> Result<DaemonState, DaemonError> {
        let all_goals = self.store.load_all_goals()?;
        let active = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Active || g.status == GoalStatus::Running)
            .count();
        let completed = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Completed)
            .count();
        let failed = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Failed)
            .count();

        Ok(DaemonState {
            started_at: Utc::now(),
            goals_completed: completed as u64,
            goals_failed: failed as u64,
            goals_active: active as u64,
            is_running: true,
            last_goal_id: None,
            total_turns: 0,
        })
    }

    /// Run the daemon main loop with periodic checkpoint and auto-rollback on failures.
    pub async fn run(&self) -> Result<(), DaemonError> {
        info!("🚀 SoulDaemon starting...");

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
            self.config.poll_interval_secs,
        ));
        let mut checkpoint_tick = tokio::time::interval(tokio::time::Duration::from_secs(300));
        let mut consecutive_failures: u64 = 0;
        let mut rollback_checkpoint: Option<String> = None;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.process_goals().await {
                        consecutive_failures += 1;
                        error!("Error processing goals ({} consecutive): {}", consecutive_failures, e);
                        if consecutive_failures >= 5 {
                            warn!("Too many consecutive failures, triggering rollback");
                            if let Some(ref cp) = rollback_checkpoint {
                                let _ = self.store.save_config("rollback_needed",
                                    &serde_json::json!({"checkpoint": cp, "timestamp": Utc::now().to_rfc3339()}));
                            }
                            consecutive_failures = 0;
                        }
                    } else {
                        consecutive_failures = 0;
                    }
                }
                _ = checkpoint_tick.tick() => {
                    // Save periodic checkpoint
                    if let Ok(goals) = self.store.load_all_goals() {
                        let cp_id = uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("cp").to_string();
                        if self.store.save_config("last_checkpoint",
                            &serde_json::json!({"id": &cp_id, "goals": goals, "ts": Utc::now().timestamp_millis()})).is_ok()
                        {
                            info!("Checkpoint saved: {}", &cp_id);
                            rollback_checkpoint = Some(cp_id);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("🛑 Shutdown signal received, stopping daemon...");
                        break;
                    }
                }
            }
        }

        info!("✅ SoulDaemon stopped gracefully");
        Ok(())
    }

    /// Process one batch of active goals
    async fn process_goals(&self) -> Result<(), DaemonError> {
        let active_goals = self.store.load_active_goals()?;

        if active_goals.is_empty() {
            return Ok(());
        }

        info!("📋 Processing {} active goals", active_goals.len());

        // Check watchdog on currently running goals
        {
            let tracker = self.goal_tracker.read().await;
            for goal in &active_goals {
                if tracker.is_running(&goal.id) {
                    if let Some(stall_count) =
                        tracker.check_stalled(&goal.id, self.config.poll_interval_secs * 6)
                    {
                        warn!(
                            "⚠️ Goal '{}' stalled (no activity for {}s, stall #{}), aborting",
                            goal.description,
                            self.config.poll_interval_secs * 6,
                            stall_count
                        );
                        self.event_bus.publish(BusEvent::SafetyWarning {
                            message: format!(
                                "Goal '{}' stalled after {} stalls, aborting",
                                goal.description, stall_count
                            ),
                            severity: "high".into(),
                        });
                        self.store
                            .fail_goal(&goal.id, &format!("Stalled ({} attempts)", stall_count))?;

                        // Auto-retry if enabled and under threshold
                        if self.config.auto_retry_failed && stall_count < 3 {
                            let mut retry_goal = goal.clone();
                            retry_goal.status = GoalStatus::Active;
                            retry_goal.updated_at = Utc::now();
                            self.store.save_goal(&retry_goal)?;
                            info!("🔄 Auto-retrying stalled goal: {}", goal.description);
                        }
                    }
                }
            }
        }

        for goal in active_goals.iter().take(self.config.max_concurrent_goals) {
            // Skip already running goals
            {
                let tracker = self.goal_tracker.read().await;
                if tracker.is_running(&goal.id) {
                    continue;
                }
            }

            info!(
                "🎯 Processing goal: {} (priority: {})",
                goal.description, goal.priority
            );

            // Mark as running
            let mut running_goal = goal.clone();
            running_goal.status = GoalStatus::Running;
            running_goal.updated_at = Utc::now();
            self.store.save_goal(&running_goal)?;

            // Register in tracker
            {
                let mut tracker = self.goal_tracker.write().await;
                tracker.start_goal(&goal.id);
            }

            // For high-complexity goals (long descriptions, high priority), use sub-agents
            let use_sub_agent = goal.description.len() > 200 || goal.priority >= 8;

            if use_sub_agent {
                info!(
                    "🤖 Goal '{}' qualifies for sub-agent decomposition",
                    goal.description
                );
                match self.execute_goal_with_sub_agents(&running_goal).await {
                    Ok(result) => {
                        info!("✅ Goal completed: {} → {}", goal.description, result);
                        self.store.complete_goal(&goal.id, &result)?;
                    }
                    Err(e) => {
                        warn!("❌ Goal failed: {} → {}", goal.description, e);
                        self.store.fail_goal(&goal.id, &e.to_string())?;
                        self.maybe_retry_goal(goal).await?;
                    }
                }
            } else {
                // Execute via ReAct loop
                match self.execute_goal(&running_goal).await {
                    Ok(result) => {
                        info!("✅ Goal completed: {} → {}", goal.description, result);
                        self.store.complete_goal(&goal.id, &result)?;
                    }
                    Err(e) => {
                        warn!("❌ Goal failed: {} → {}", goal.description, e);
                        self.store.fail_goal(&goal.id, &e.to_string())?;
                        self.maybe_retry_goal(goal).await?;
                    }
                }
            }

            // Clean up tracker
            {
                let mut tracker = self.goal_tracker.write().await;
                tracker.finish_goal(&goal.id);
            }
        }

        Ok(())
    }

    async fn maybe_retry_goal(&self, goal: &PersistentGoal) -> Result<(), DaemonError> {
        if self.config.auto_retry_failed {
            let mut retry_goal = goal.clone();
            retry_goal.status = GoalStatus::Active;
            retry_goal.updated_at = Utc::now();
            self.store.save_goal(&retry_goal)?;
            info!("🔄 Retrying goal: {}", goal.description);
        }
        Ok(())
    }

    async fn execute_goal(&self, goal: &PersistentGoal) -> Result<String, DaemonError> {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let config = soul_agent_core::AgentConfig {
            max_turns: self.config.max_turns_per_goal,
            ..Default::default()
        };

        let llm = OllamaClient::new(self.llm_config.clone());
        let mut agent = AutonomousAgent::new(llm, config);
        if let Some(path) = &self.config.provenance_path {
            // A missing file is a fresh start (Ok); an unreadable index is
            // worth failing the goal over — running anyway would persist a
            // fresh index over the evidence at the end of the run.
            agent.load_memory_provenance(path).map_err(|e| {
                DaemonError::Agent(format!(
                    "provenance index at {} is unreadable: {e}",
                    path.display()
                ))
            })?;
        }
        agent.set_event_sender(event_tx);

        // Publish task received event
        self.event_bus.publish(BusEvent::TaskReceived {
            task_id: goal.id.clone(),
            description: goal.description.clone(),
        });

        let handle = {
            let description = goal.description.clone();
            let mut agent = agent;
            let persist_provenance = self.config.provenance_path.is_some();
            tokio::spawn(async move {
                let result = agent.run_task(&description).await;
                // Persist on failure too: what the agent observed during a
                // failed run is still evidence, and losing it makes the same
                // records come back `Unknown` after a restart.
                if persist_provenance {
                    if let Err(e) = agent.persist_memory_provenance() {
                        tracing::warn!("memory-provenance persist failed: {e}");
                    }
                }
                result
            })
        };

        // Collect events and forward to event bus + tracker
        while let Some(event) = event_rx.recv().await {
            match &event {
                AgentEvent::Thinking { content, .. } => {
                    info!("💭 Thought: {}", content);
                    self.event_bus.publish(BusEvent::Thought {
                        turn: 0,
                        content: content.clone(),
                    });
                    let mut tracker = self.goal_tracker.write().await;
                    let turn = tracker.running.get(&goal.id).map(|t| t.1 + 1).unwrap_or(0);
                    tracker.update_activity(&goal.id, turn);
                }
                AgentEvent::ToolCall { name, args, .. } => {
                    info!("🔧 Tool: {} → {}", name, args);
                    self.event_bus.publish(BusEvent::ToolCall {
                        turn: 0,
                        tool: name.clone(),
                        input: args.to_string(),
                    });
                    let mut tracker = self.goal_tracker.write().await;
                    let turn = tracker.running.get(&goal.id).map(|t| t.1 + 1).unwrap_or(0);
                    tracker.update_activity(&goal.id, turn);
                }
                AgentEvent::ToolResult {
                    name,
                    output,
                    success,
                    ..
                } => {
                    let prefix = if *success { "✅" } else { "❌" };
                    if output.len() > 200 {
                        info!(
                            "{} Result {}: {}...[{} more chars]",
                            prefix,
                            name,
                            &output[..200],
                            output.len() - 200
                        );
                    } else {
                        info!("{} Result {}: {}", prefix, name, output);
                    }
                    self.event_bus.publish(BusEvent::ToolResult {
                        turn: 0,
                        tool: name.clone(),
                        output: output.clone(),
                        success: *success,
                    });
                }
                AgentEvent::SafetyWarning { message, .. } => {
                    warn!("⚠️ Safety: {}", message);
                    self.event_bus.publish(BusEvent::SafetyWarning {
                        message: message.clone(),
                        severity: "high".into(),
                    });
                }
                AgentEvent::Done { summary, .. } => {
                    info!("🏁 Done: {}", summary);
                    self.event_bus.publish(BusEvent::TaskCompleted {
                        task_id: goal.id.clone(),
                        result: summary.clone(),
                        duration_ms: 0,
                    });
                }
                AgentEvent::Error { message, .. } => {
                    error!("Error: {}", message);
                    self.event_bus.publish(BusEvent::TaskFailed {
                        task_id: goal.id.clone(),
                        error: message.clone(),
                    });
                }
                _ => {}
            }
        }

        let result = handle
            .await
            .map_err(|e| DaemonError::Agent(e.to_string()))?;
        result.map_err(DaemonError::Agent)
    }

    /// Execute a complex goal by decomposing into sub-tasks via sub-agents
    async fn execute_goal_with_sub_agents(
        &self,
        goal: &PersistentGoal,
    ) -> Result<String, DaemonError> {
        // Use LLM to decompose the goal into sub-tasks
        let llm = OllamaClient::new(self.llm_config.clone());
        let decomposition_prompt = format!(
            "Decompose this task into 2-4 independent sub-tasks. \
             Return ONLY a JSON array of strings, each string is a sub-task.\n\nTask: {}",
            goal.description
        );

        let response = llm
            .chat(
                &[soul_llm::ChatMessage {
                    role: soul_llm::Role::User,
                    content: decomposition_prompt,
                    tool_calls: None,
                    tool_call_id: None,
                }],
                None,
            )
            .await
            .map_err(|e| DaemonError::Llm(e.to_string()))?;

        let response_text = response.message.content.unwrap_or_default();

        // Parse sub-tasks from response
        let sub_tasks: Vec<String> = if let Some(start) = response_text.find('[') {
            if let Some(end) = response_text[start..].find(']') {
                let json_str = &response_text[start..=start + end];
                serde_json::from_str::<Vec<String>>(json_str).unwrap_or_else(|_| {
                    // Fallback: split by newlines
                    response_text
                        .lines()
                        .filter(|l| {
                            !l.trim().is_empty() && !l.starts_with('[') && !l.starts_with(']')
                        })
                        .map(|l| {
                            l.trim_matches(|c: char| c == '"' || c == '-' || c == ' ')
                                .to_string()
                        })
                        .filter(|l| !l.is_empty())
                        .take(4)
                        .collect()
                })
            } else {
                vec![goal.description.clone()]
            }
        } else {
            vec![goal.description.clone()]
        };

        info!(
            "📐 Decomposed goal '{}' into {} sub-tasks",
            goal.description,
            sub_tasks.len()
        );

        self.event_bus.publish(BusEvent::Thought {
            turn: 0,
            content: format!(
                "Decomposed into {} sub-tasks: {:?}",
                sub_tasks.len(),
                sub_tasks
                    .iter()
                    .map(|s| &s[..s.len().min(50)])
                    .collect::<Vec<_>>()
            ),
        });

        // Spawn sub-agents for each sub-task
        let mut task_ids = Vec::new();
        for sub_task in &sub_tasks {
            match self.sub_agents.spawn(sub_task, Some(&goal.id)).await {
                Ok(task_id) => {
                    task_ids.push((task_id, sub_task.clone()));
                }
                Err(e) => {
                    warn!("Failed to spawn sub-agent for '{}': {}", sub_task, e);
                }
            }
        }

        // Wait for all sub-agents
        let mut results = Vec::new();
        for (task_id, sub_task) in &task_ids {
            match self.sub_agents.wait_for(task_id, 300).await {
                Ok(status) => match status.status {
                    soul_subagents::TaskStatus::Completed => {
                        let result = status.result.unwrap_or_default();
                        results.push(format!(
                            "✅ '{}' → {}",
                            sub_task,
                            &result[..result.len().min(200)]
                        ));
                    }
                    _ => {
                        let err = status.error.unwrap_or_else(|| "unknown".into());
                        results.push(format!("❌ '{}' → {}", sub_task, err));
                    }
                },
                Err(e) => {
                    results.push(format!("❌ '{}' → timeout/error: {}", sub_task, e));
                }
            }
        }

        let summary = results.join("\n");
        self.event_bus.publish(BusEvent::TaskCompleted {
            task_id: goal.id.clone(),
            result: summary.clone(),
            duration_ms: 0,
        });

        Ok(summary)
    }

    /// Shutdown the daemon gracefully
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Spawn a sub-agent for a sub-task
    pub async fn spawn_sub_agent(
        &self,
        description: &str,
        parent_goal_id: Option<&str>,
    ) -> Result<String, DaemonError> {
        let task_id = self
            .sub_agents
            .spawn(description, parent_goal_id)
            .await
            .map_err(|e| DaemonError::Agent(e.to_string()))?;

        info!("🤖 Sub-agent spawned: {} ({})", description, &task_id[..8]);
        Ok(task_id)
    }

    /// Get sub-agent manager for direct access
    pub fn sub_agents(&self) -> &SubAgentManager {
        &self.sub_agents
    }

    /// Wait for a sub-agent to complete
    pub async fn wait_sub_agent(
        &self,
        task_id: &str,
        timeout_secs: u64,
    ) -> Result<String, DaemonError> {
        let result = self
            .sub_agents
            .wait_for(task_id, timeout_secs)
            .await
            .map_err(|e| DaemonError::Agent(e.to_string()))?;

        match result.status {
            soul_subagents::TaskStatus::Completed => Ok(result.result.unwrap_or_default()),
            _ => Err(DaemonError::Agent(
                result.error.unwrap_or_else(|| "Sub-agent failed".into()),
            )),
        }
    }

    /// Collect all completed sub-agent results
    pub async fn collect_sub_agent_results(&self) -> Vec<(String, String)> {
        self.sub_agents.collect_results().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();
        assert_eq!(config.poll_interval_secs, 5);
        assert_eq!(config.max_turns_per_goal, 20);
    }

    #[test]
    fn test_daemon_config_custom() {
        let config = DaemonConfig {
            poll_interval_secs: 10,
            max_turns_per_goal: 50,
            ..DaemonConfig::default()
        };
        assert_eq!(config.poll_interval_secs, 10);
        assert_eq!(config.max_turns_per_goal, 50);
    }

    #[test]
    fn test_daemon_add_goal() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        let goal = daemon.add_goal("test task", 5).unwrap();
        assert_eq!(goal.description, "test task");
        assert_eq!(goal.priority, 5);
    }

    #[test]
    fn test_daemon_add_goal_zero_priority() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        let goal = daemon.add_goal("low priority", 0).unwrap();
        assert_eq!(goal.priority, 0);
    }

    #[test]
    fn test_daemon_add_goal_high_priority() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        let goal = daemon.add_goal("critical", 10).unwrap();
        assert_eq!(goal.priority, 10);
    }

    #[test]
    fn test_daemon_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        daemon.add_goal("goal1", 1).unwrap();
        let state = daemon.state().unwrap();
        assert_eq!(state.goals_active, 1);
    }

    #[test]
    fn test_daemon_state_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        let state = daemon.state().unwrap();
        assert_eq!(state.goals_active, 0);
        assert_eq!(state.goals_completed, 0);
        assert_eq!(state.goals_failed, 0);
    }

    #[test]
    fn test_daemon_multiple_goals() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        daemon.add_goal("goal1", 1).unwrap();
        daemon.add_goal("goal2", 2).unwrap();
        daemon.add_goal("goal3", 3).unwrap();
        let state = daemon.state().unwrap();
        assert_eq!(state.goals_active, 3);
    }

    #[test]
    fn test_daemon_event_bus() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        let _bus = daemon.event_bus();
    }

    #[test]
    fn test_daemon_sub_agents() {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        let daemon = Daemon::new(DaemonConfig::default(), store);

        let _agents = daemon.sub_agents();
    }
}
