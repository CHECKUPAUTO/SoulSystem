#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use avid_core::queue::{RedisQueue, TaskMessage, TaskQueue};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A distributed crawl task sent from master to workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlTask {
    pub url: String,
    pub depth: u32,
    pub worker_id: String,
}

/// A result produced by a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub url: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Master that pushes crawl tasks to a Redis queue.
pub struct ScoutMaster {
    queue: RedisQueue,
}

impl ScoutMaster {
    /// Connect to Redis and create a master.
    pub async fn new(redis_url: &str) -> Result<Self, avid_core::queue::QueueError> {
        let queue = RedisQueue::new(redis_url).await?;
        Ok(Self { queue })
    }

    /// Push a crawl task onto the distributed queue.
    pub async fn enqueue(
        &self,
        url: &str,
        depth: u32,
        worker_id: &str,
    ) -> Result<(), avid_core::queue::QueueError> {
        let task = CrawlTask {
            url: url.to_string(),
            depth,
            worker_id: worker_id.to_string(),
        };
        let payload = serde_json::to_string(&task)?;
        let now: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .try_into()
            .unwrap_or(i64::MAX);
        let msg = TaskMessage {
            task_id: format!("scout-{url}"),
            task_text: payload,
            task_type: None,
            url: Some(url.to_string()),
            metadata: None,
            enqueued_at: now,
        };
        self.queue.put(&msg).await
    }
}

/// Worker that pulls tasks from Redis.
pub struct ScoutWorker;

impl ScoutWorker {
    /// Pop the next crawl task from the queue.
    pub async fn pop_task(
        queue: &dyn TaskQueue,
        timeout_secs: u64,
    ) -> Result<Option<CrawlTask>, avid_core::queue::QueueError> {
        let msg = queue.pop(Duration::from_secs(timeout_secs)).await?;
        match msg {
            Some(m) => {
                let task: CrawlTask = serde_json::from_str(&m.task_text)?;
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }
}
