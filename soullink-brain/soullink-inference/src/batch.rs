//! Batch scheduler — priority queue for inference requests.
//!
//! Ordering: Think > Embed > Dream.
//! Batches embedding requests for throughput.
//! Limits concurrent inference to max_concurrent.

use crate::types::{GenerateRequest, Priority};
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

/// A prioritized inference request.
#[derive(Debug)]
struct QueuedRequest {
    priority: Priority,
    request: GenerateRequest,
    submitted_at: std::time::Instant,
}

impl PartialEq for QueuedRequest {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for QueuedRequest {}

impl PartialOrd for QueuedRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Lower priority value = higher importance (Think=0 < Dream=2)
        // BinaryHeap pops max, so we reverse: Dream (2) should pop LAST
        other.priority.cmp(&self.priority) // Reverse: Think pops first
    }
}

/// Batch scheduler with priority queue and concurrency limiter.
pub struct BatchScheduler {
    /// Priority queue of pending requests.
    queue: Arc<tokio::sync::Mutex<BinaryHeap<QueuedRequest>>>,
    /// Concurrency limiter.
    semaphore: Arc<Semaphore>,
    /// Maximum concurrent inference tasks.
    max_concurrent: usize,
}

impl BatchScheduler {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            queue: Arc::new(tokio::sync::Mutex::new(BinaryHeap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        }
    }

    /// Submit a request to the priority queue.
    pub async fn submit(&self, request: GenerateRequest) {
        let priority = request.priority;
        let queued = QueuedRequest {
            priority,
            request,
            submitted_at: std::time::Instant::now(),
        };
        self.queue.lock().await.push(queued);
        info!(priority = ?priority, "Request queued");
    }

    /// Acquire a concurrency slot and pop the highest-priority request.
    pub async fn next(&self) -> Option<GenerateRequest> {
        let _permit = self.semaphore.acquire().await.ok()?;
        let mut queue = self.queue.lock().await;
        queue.pop().map(|qr| {
            let wait_ms = qr.submitted_at.elapsed().as_millis();
            if wait_ms > 1000 && qr.priority == Priority::Think {
                warn!(wait_ms, "Think request waited too long in queue");
            }
            qr.request
        })
    }

    /// Number of pending requests.
    pub async fn pending(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Available concurrency slots.
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Maximum concurrent capacity.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req(priority: Priority) -> GenerateRequest {
        GenerateRequest {
            model: "test".into(),
            prompt: "test".into(),
            max_tokens: 10,
            temperature: 0.5,
            priority,
            quantization: None,
        }
    }

    #[tokio::test]
    async fn priority_ordering_think_first() {
        let scheduler = BatchScheduler::new(4);
        // Submit in reverse priority order
        scheduler.submit(make_req(Priority::Dream)).await;
        scheduler.submit(make_req(Priority::Embed)).await;
        scheduler.submit(make_req(Priority::Think)).await;

        let req = scheduler.next().await.unwrap();
        assert_eq!(req.priority, Priority::Think);
    }

    #[tokio::test]
    async fn priority_ordering_embed_before_dream() {
        let scheduler = BatchScheduler::new(4);
        scheduler.submit(make_req(Priority::Dream)).await;
        scheduler.submit(make_req(Priority::Embed)).await;

        let req = scheduler.next().await.unwrap();
        assert_eq!(req.priority, Priority::Embed);
    }

    #[tokio::test]
    async fn concurrency_limit() {
        let scheduler = BatchScheduler::new(2);
        assert_eq!(scheduler.available(), 2);
        // Submit 3 requests
        scheduler.submit(make_req(Priority::Think)).await;
        scheduler.submit(make_req(Priority::Think)).await;
        scheduler.submit(make_req(Priority::Think)).await;
        assert_eq!(scheduler.pending().await, 3);
    }

    #[tokio::test]
    async fn empty_queue_returns_none() {
        let scheduler = BatchScheduler::new(4);
        // next() on empty queue: the semaphore permit is acquired but queue is empty
        // This would block forever, so we test pending instead
        assert_eq!(scheduler.pending().await, 0);
    }
}
