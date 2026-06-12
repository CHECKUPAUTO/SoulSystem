use garde::Validate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use crate::agents::base::AgentBase;
use crate::errors::CoreError;
use crate::queue::{TaskMessage, TaskQueue};

const SYSTEM_PROMPT: &str = "\
You are a task decomposition expert. Your role is to split a complex engineering task into 3-8 atomic, sequential sub-tasks.

Each sub-task must be a self-contained unit of work that can be executed by another agent.
The sub-tasks should be ordered such that they logically build towards the final goal.

Output a JSON object with:
- subtasks: array of {description: string}

Output only valid JSON.";

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct Subtask {
    #[garde(length(min = 1, max = 500))]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SplitResponse {
    #[garde(length(min = 3, max = 8), dive)]
    pub subtasks: Vec<Subtask>,
}

/// The Splitter agent decomposes a complex task into smaller sub-tasks.
pub struct Splitter {
    base: AgentBase,
    queue: Arc<Box<dyn TaskQueue>>,
}

impl Splitter {
    #[must_use]
    pub fn new(base: AgentBase, queue: Arc<Box<dyn TaskQueue>>) -> Self {
        Self { base, queue }
    }

    /// Split a task and enqueue the sub-tasks.
    ///
    /// # Errors
    /// Returns `CoreError::Agent` if planning fails or queue is unreachable.
    pub async fn split(&self, parent_task_id: &str, task_text: &str) -> Result<(), CoreError> {
        let user_prompt = format!("Task to decompose:\n{task_text}");

        let response: SplitResponse = self
            .base
            .chat_and_parse(SYSTEM_PROMPT, &user_prompt, Duration::from_secs(60), 2)
            .await?;

        let mut subtask_ids = Vec::new();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();

        for (i, subtask) in response.subtasks.iter().enumerate() {
            let subtask_id = format!("{parent_task_id}-sub-{}", i + 1);
            subtask_ids.push(subtask_id.clone());

            let metadata = serde_json::json!({
                "parent_task_id": parent_task_id,
                "subtask_index": i,
                "total_subtasks": response.subtasks.len(),
            });

            let msg = TaskMessage {
                task_id: subtask_id,
                task_text: subtask.description.clone(),
                task_type: None,
                url: None,
                metadata: Some(metadata),
                enqueued_at: now,
            };

            self.queue
                .put(&msg)
                .await
                .map_err(|e| CoreError::Agent(format!("queue error: {e}")))?;
        }

        // Enqueue aggregator task
        let aggregator_id = format!("{parent_task_id}-aggregator");
        let agg_metadata = serde_json::json!({
            "is_aggregator": true,
            "parent_task_id": parent_task_id,
            "subtask_ids": subtask_ids,
        });

        let agg_msg = TaskMessage {
            task_id: aggregator_id,
            task_text: format!("Aggregate results for task {parent_task_id}"),
            task_type: None,
            url: None,
            metadata: Some(agg_metadata),
            enqueued_at: now,
        };

        self.queue
            .put(&agg_msg)
            .await
            .map_err(|e| CoreError::Agent(format!("queue error: {e}")))?;

        Ok(())
    }
}
