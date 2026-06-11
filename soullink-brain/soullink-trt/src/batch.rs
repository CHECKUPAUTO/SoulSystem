//! Batch scheduler — priority-based batching for TRT inference.
//!
//! Think requests are GPU-bound and high priority.
//! Dream requests are CPU-bound and low priority.
//! Embed requests are GPU-bound but small and batchable.

use crate::types::{BatchItem, BatchPriority};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Priority-based batch scheduler.
///
/// Think requests get immediate GPU access. Dream requests wait for idle slots.
/// Embed requests batch together for throughput.
pub struct BatchScheduler {
    /// Maximum concurrent inference slots.
    max_concurrent: usize,
    /// Semaphore for concurrency control.
    semaphore: Arc<Semaphore>,
    /// Think queue (high priority).
    think_queue: Arc<tokio::sync::Mutex<VecDeque<BatchItem>>>,
    /// Dream queue (low priority).
    dream_queue: Arc<tokio::sync::Mutex<VecDeque<BatchItem>>>,
    /// Embed queue (medium priority, batchable).
    embed_queue: Arc<tokio::sync::Mutex<VecDeque<BatchItem>>>,
}

impl BatchScheduler {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            think_queue: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
            dream_queue: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
            embed_queue: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
        }
    }

    /// Submit a batch item to the appropriate queue.
    pub async fn submit(&self, item: BatchItem) {
        match item.priority {
            BatchPriority::Think => self.think_queue.lock().await.push_back(item),
            BatchPriority::Embed => self.embed_queue.lock().await.push_back(item),
            BatchPriority::Dream => self.dream_queue.lock().await.push_back(item),
        }
    }

    /// Acquire a slot for inference (respects concurrency limits).
    pub async fn acquire_slot(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.semaphore.acquire().await.expect("semaphore closed")
    }

    /// Pop the next item by priority: Think → Embed → Dream.
    pub async fn pop_next(&self) -> Option<BatchItem> {
        if let Some(item) = self.think_queue.lock().await.pop_front() {
            return Some(item);
        }
        if let Some(item) = self.embed_queue.lock().await.pop_front() {
            return Some(item);
        }
        self.dream_queue.lock().await.pop_front()
    }

    /// Drain all embed items for batch processing.
    pub async fn drain_embeds(&self, max_batch: usize) -> Vec<BatchItem> {
        let mut queue = self.embed_queue.lock().await;
        let count = max_batch.min(queue.len());
        queue.drain(..count).collect()
    }

    /// Queue lengths by priority.
    pub async fn queue_lengths(&self) -> (usize, usize, usize) {
        let think = self.think_queue.lock().await.len();
        let embed = self.embed_queue.lock().await.len();
        let dream = self.dream_queue.lock().await.len();
        (think, embed, dream)
    }

    /// Total items across all queues.
    pub async fn total_queued(&self) -> usize {
        let (t, e, d) = self.queue_lengths().await;
        t + e + d
    }

    /// Available concurrency slots.
    pub fn available_slots(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GenerationConfig;

    fn make_item(priority: BatchPriority) -> BatchItem {
        BatchItem {
            request_id: uuid::Uuid::new_v4().to_string(),
            prompt: "test prompt".to_string(),
            config: GenerationConfig::default(),
            priority,
        }
    }

    #[tokio::test]
    async fn submit_and_pop_by_priority() {
        let scheduler = BatchScheduler::new(4);

        scheduler.submit(make_item(BatchPriority::Dream)).await;
        scheduler.submit(make_item(BatchPriority::Think)).await;
        scheduler.submit(make_item(BatchPriority::Embed)).await;

        // Think comes first
        let first = scheduler.pop_next().await.unwrap();
        assert_eq!(first.priority, BatchPriority::Think);

        let second = scheduler.pop_next().await.unwrap();
        assert_eq!(second.priority, BatchPriority::Embed);

        let third = scheduler.pop_next().await.unwrap();
        assert_eq!(third.priority, BatchPriority::Dream);
    }

    #[tokio::test]
    async fn drain_embeds_batch() {
        let scheduler = BatchScheduler::new(4);
        for _ in 0..5 {
            scheduler.submit(make_item(BatchPriority::Embed)).await;
        }

        let batch = scheduler.drain_embeds(3).await;
        assert_eq!(batch.len(), 3);
        assert_eq!(scheduler.total_queued().await, 2);
    }

    #[tokio::test]
    async fn queue_lengths() {
        let scheduler = BatchScheduler::new(4);
        scheduler.submit(make_item(BatchPriority::Think)).await;
        scheduler.submit(make_item(BatchPriority::Think)).await;
        scheduler.submit(make_item(BatchPriority::Embed)).await;
        scheduler.submit(make_item(BatchPriority::Dream)).await;

        let (t, e, d) = scheduler.queue_lengths().await;
        assert_eq!(t, 2);
        assert_eq!(e, 1);
        assert_eq!(d, 1);
    }

    #[tokio::test]
    async fn semaphore_controls_concurrency() {
        let scheduler = BatchScheduler::new(2);
        assert_eq!(scheduler.available_slots(), 2);

        let _slot1 = scheduler.acquire_slot().await;
        assert_eq!(scheduler.available_slots(), 1);

        let _slot2 = scheduler.acquire_slot().await;
        assert_eq!(scheduler.available_slots(), 0);
    }

    #[tokio::test]
    async fn max_concurrent() {
        let scheduler = BatchScheduler::new(8);
        assert_eq!(scheduler.max_concurrent(), 8);
    }
}
