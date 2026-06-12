use crate::agent::TaskResult;
use crate::MultiAgentConfig;
use moka::future::Cache as MokaCache;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

/// Thread-safe task result cache backed by moka.
pub struct TaskCache {
    inner: Arc<MokaCache<String, TaskResult>>,
}

impl TaskCache {
    /// Create a new TaskCache with the configured TTL.
    pub fn new(config: MultiAgentConfig) -> Self {
        let cache: MokaCache<String, TaskResult> = MokaCache::builder()
            .time_to_live(Duration::from_secs(config.cache_ttl_secs))
            .build();
        Self {
            inner: Arc::new(cache),
        }
    }

    /// Insert a value into the cache.
    pub async fn set(&self, key: String, value: TaskResult) {
        debug!("Caching result for key: {}", key);
        self.inner.insert(key, value).await;
    }

    /// Retrieve a value from the cache, if present.
    pub fn get(&self, key: &str) -> Option<TaskResult> {
        let value = self.inner.get(key);
        if value.is_some() {
            debug!("Cache hit for key: {}", key);
        }
        value
    }

    /// Check whether a key exists in the cache.
    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Remove a key from the cache.
    pub async fn invalidate(&self, key: &str) {
        self.inner.invalidate(key).await;
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        self.inner.invalidate_all();
    }
}
