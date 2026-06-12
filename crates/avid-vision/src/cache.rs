use crate::types::VisionResult;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// A memory-bounded LRU cache for vision inference results.
#[derive(Debug)]
pub struct InferenceCache {
    cache: Mutex<LruCache<String, VisionResult>>,
}

impl InferenceCache {
    /// Create a new cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(100).unwrap()),
            )),
        }
    }

    /// Get a result from the cache.
    pub fn get(&self, key: &str) -> Option<VisionResult> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(key).cloned()
    }

    /// Put a result into the cache.
    pub fn put(&self, key: String, result: VisionResult) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(key, result);
    }

    /// Clear the cache.
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
}

impl Default for InferenceCache {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SceneDescription;
    use chrono::Utc;

    #[test]
    fn test_cache_put_get() {
        let cache = InferenceCache::new(2);
        let result = VisionResult {
            elements: vec![],
            scene: Some(SceneDescription {
                caption: "test".to_string(),
                tags: vec![],
                confidence: 0.9,
            }),
            timestamp: Utc::now(),
            dimensions: (1920, 1080),
        };

        cache.put("key1".to_string(), result.clone());
        let cached = cache.get("key1").expect("should be in cache");
        assert_eq!(cached.scene.unwrap().caption, "test");
    }

    #[test]
    fn test_cache_clear() {
        let cache = InferenceCache::new(10);
        let result = VisionResult {
            elements: vec![],
            scene: None,
            timestamp: Utc::now(),
            dimensions: (100, 100),
        };
        cache.put("k".to_string(), result);
        cache.clear();
        assert!(cache.get("k").is_none());
    }
}
