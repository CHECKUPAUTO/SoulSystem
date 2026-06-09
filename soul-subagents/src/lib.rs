//! Sub-agent spawning and task delegation system.
//!
//! Enables the main agent to spawn worker agents for parallel tasks,
//! monitor their progress, and collect results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use soul_agent_core::AgentEvent;
use soul_llm::LlmConfig;

#[derive(Error, Debug)]
pub enum SubAgentError {
    #[error("Spawn failed: {0}")]
    SpawnFailed(String),
    #[error("Task not found: {0}")]
    NotFound(String),
    #[error("Agent crashed: {0}")]
    Crashed(String),
    #[error("Timeout")]
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentTask {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub worker_id: Option<String>,
    pub parent_id: Option<String>,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub struct SubAgentManager {
    tasks: Arc<RwLock<HashMap<String, SubAgentTask>>>,
    llm_config: soul_llm::LlmConfig,
    max_concurrent: usize,
    active_workers: Arc<std::sync::atomic::AtomicUsize>,
}

impl SubAgentManager {
    pub fn new(llm_config: soul_llm::LlmConfig, max_concurrent: usize) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            llm_config,
            max_concurrent,
            active_workers: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Spawn a sub-agent for a task
    pub async fn spawn(
        &self,
        description: &str,
        parent_id: Option<&str>,
    ) -> Result<String, SubAgentError> {
        let current = self
            .active_workers
            .load(std::sync::atomic::Ordering::SeqCst);
        if current >= self.max_concurrent {
            return Err(SubAgentError::SpawnFailed(format!(
                "Max concurrent workers ({}) reached",
                self.max_concurrent
            )));
        }

        let task_id = Uuid::new_v4().to_string();
        let task = SubAgentTask {
            id: task_id.clone(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
            worker_id: None,
            parent_id: parent_id.map(|s| s.to_string()),
            progress: 0.0,
        };

        self.tasks
            .write()
            .await
            .insert(task_id.clone(), task);

        // Spawn the worker task
        let tasks = self.tasks.clone();
        let llm_config = self.llm_config.clone();
        let active_workers = self.active_workers.clone();
        let tid = task_id.clone();
        let desc = description.to_string();

        active_workers.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            // Mark as running
            {
                let mut tasks = tasks.write().await;
                if let Some(task) = tasks.get_mut(&tid) {
                    task.status = TaskStatus::Running;
                    task.started_at = Some(Utc::now());
                }
            }

            // Execute via simple task
            let result = execute_simple_task(&llm_config, &desc).await;

            // Update task status
            {
                let mut tasks = tasks.write().await;
                if let Some(task) = tasks.get_mut(&tid) {
                    match result {
                        Ok(output) => {
                            task.status = TaskStatus::Completed;
                            task.result = Some(output);
                            task.progress = 1.0;
                        }
                        Err(e) => {
                            task.status = TaskStatus::Failed;
                            task.error = Some(e);
                        }
                    }
                    task.completed_at = Some(Utc::now());
                }
            }

            active_workers.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });

        Ok(task_id)
    }

    /// Get task status
    pub async fn get_task(&self, task_id: &str) -> Result<SubAgentTask, SubAgentError> {
        let tasks = self.tasks.read().await;
        tasks
            .get(task_id)
            .cloned()
            .ok_or_else(|| SubAgentError::NotFound(task_id.to_string()))
    }

    /// List all tasks
    pub async fn list_tasks(&self) -> Vec<SubAgentTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    /// List active tasks
    pub async fn active_tasks(&self) -> Vec<SubAgentTask> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.status == TaskStatus::Running || t.status == TaskStatus::Pending)
            .cloned()
            .collect()
    }

    /// Cancel a task
    pub async fn cancel(&self, task_id: &str) -> Result<(), SubAgentError> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = TaskStatus::Cancelled;
            task.completed_at = Some(Utc::now());
            Ok(())
        } else {
            Err(SubAgentError::NotFound(task_id.to_string()))
        }
    }

    /// Get results of all completed tasks
    pub async fn collect_results(&self) -> Vec<(String, String)> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .filter_map(|t| {
                t.result
                    .as_ref()
                    .map(|r| (t.description.clone(), r.clone()))
            })
            .collect()
    }

    /// Wait for a specific task to complete
    pub async fn wait_for(
        &self,
        task_id: &str,
        timeout_secs: u64,
    ) -> Result<SubAgentTask, SubAgentError> {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(timeout_secs);

        loop {
            let task = self.get_task(task_id).await?;
            if task.status == TaskStatus::Completed
                || task.status == TaskStatus::Failed
                || task.status == TaskStatus::Cancelled
            {
                return Ok(task);
            }
            if std::time::Instant::now() >= deadline {
                return Err(SubAgentError::Timeout);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

async fn execute_simple_task(llm_config: &soul_llm::LlmConfig, description: &str) -> Result<String, String> {
    use soul_llm::{ChatMessage, Role};

    let messages = vec![
        ChatMessage {
            role: Role::System,
            content: "You are a worker agent. Complete tasks concisely.".to_string(),
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage {
            role: Role::User,
            content: description.to_string(),
            tool_calls: None,
            tool_call_id: None,
        },
    ];

    let llm = soul_llm::OllamaClient::new(llm_config.clone());
    let response = llm
        .chat(&messages, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.message.content.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status() {
        let task = SubAgentTask {
            id: "test".into(),
            description: "test task".into(),
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
            worker_id: None,
            parent_id: None,
            progress: 0.0,
        };
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[test]
    fn test_manager_creation() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 4);
        assert_eq!(manager.max_concurrent, 4);
    }

    #[tokio::test]
    async fn test_spawn_fails_when_max_concurrent_reached() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 0);

        let result = manager.spawn("task 1", None).await;
        assert!(result.is_err());
        match result {
            Err(SubAgentError::SpawnFailed(msg)) => assert!(msg.contains("Max concurrent")),
            _ => panic!("Expected SpawnFailed"),
        }
    }

    #[tokio::test]
    async fn test_list_tasks_after_spawn() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        let id1 = manager.spawn("task 1", None).await.unwrap();
        let id2 = manager.spawn("task 2", None).await.unwrap();

        let tasks = manager.list_tasks().await;
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.id == id1));
        assert!(tasks.iter().any(|t| t.id == id2));
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        let id = manager.spawn("cancel task", None).await.unwrap();

        manager.cancel(&id).await.unwrap();

        let task = manager.get_task(&id).await.unwrap();
        assert_eq!(task.status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        let result = manager.get_task("nonexistent-id").await;
        match result {
            Err(SubAgentError::NotFound(id)) => assert_eq!(id, "nonexistent-id"),
            _ => panic!("Expected NotFound"),
        }
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_task() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        let result = manager.cancel("nonexistent-id").await;
        match result {
            Err(SubAgentError::NotFound(id)) => assert_eq!(id, "nonexistent-id"),
            _ => panic!("Expected NotFound"),
        }
    }

    #[tokio::test]
    async fn test_active_tasks_filter() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        let id = manager.spawn("active task", None).await.unwrap();

        let active = manager.active_tasks().await;
        assert!(active.iter().any(|t| t.id == id));

        manager.cancel(&id).await.unwrap();

        let active = manager.active_tasks().await;
        assert!(!active.iter().any(|t| t.id == id));
    }

    #[tokio::test]
    async fn test_collect_results_empty_when_no_completed() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        manager.spawn("result task", None).await.unwrap();

        let results = manager.collect_results().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_task_default_progress() {
        let config = soul_llm::LlmConfig::default();
        let manager = SubAgentManager::new(config, 5);

        let id = manager.spawn("progress test", None).await.unwrap();
        let task = manager.get_task(&id).await.unwrap();
        assert_eq!(task.progress, 0.0);
        assert_eq!(task.status, TaskStatus::Pending);
    }
}
