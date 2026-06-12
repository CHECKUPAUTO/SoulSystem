use std::sync::atomic::{AtomicU64, Ordering};

/// Simple counters for ScoutEngine operations.
#[derive(Debug, Default)]
pub struct ScoutMetrics {
    pub pages_crawled: AtomicU64,
    pub pages_cached: AtomicU64,
    pub requests_failed: AtomicU64,
    pub robots_blocked: AtomicU64,
    pub domains_throttled: AtomicU64,
}

impl ScoutMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_pages_crawled(&self) {
        self.pages_crawled.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_pages_cached(&self) {
        self.pages_cached.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_requests_failed(&self) {
        self.requests_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_robots_blocked(&self) {
        self.robots_blocked.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_domains_throttled(&self) {
        self.domains_throttled.fetch_add(1, Ordering::Relaxed);
    }
}
