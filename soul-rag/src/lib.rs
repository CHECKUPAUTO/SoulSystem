//! RAG integration combining web fetch, browser, and memory.
//!
//! Provides:
//! - Unified search across web content and local memory
//! - Content fetching with automatic extraction
//! - Browser-based JavaScript rendering for dynamic content
//! - Persistent storage of fetched content
//! - Relevance scoring and ranking

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RagError {
    #[error("Fetch error: {0}")]
    Fetch(#[from] soul_webfetch::FetchError),
    #[error("Browser error: {0}")]
    Browser(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("No results found")]
    NoResults,
}

// ── RAG Content ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagContent {
    pub id: String,
    pub url: String,
    pub title: String,
    pub text: String,
    pub metadata: HashMap<String, String>,
    pub fetched_at: DateTime<Utc>,
    pub source_type: SourceType,
    pub relevance_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    WebFetch,
    Browser,
    LocalMemory,
    Document,
}

// ── RAG Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RagConfig {
    pub max_results: usize,
    pub min_relevance: f32,
    pub fetch_timeout_ms: u64,
    pub use_browser_fallback: bool,
    pub cache_size: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            max_results: 10,
            min_relevance: 0.5,
            fetch_timeout_ms: 30000,
            use_browser_fallback: true,
            cache_size: 100,
        }
    }
}

// ── RAG Store ─────────────────────────────────────────────────────────

pub struct RagStore {
    config: RagConfig,
    fetcher: soul_webfetch::WebFetcher,
    cache: HashMap<String, RagContent>,
}

impl RagStore {
    pub fn new(config: RagConfig) -> Self {
        let fetcher = soul_webfetch::WebFetcher::new(soul_webfetch::FetcherConfig {
            timeout_ms: config.fetch_timeout_ms,
            ..Default::default()
        })
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to create WebFetcher: {e}, using default");
            soul_webfetch::WebFetcher::new(soul_webfetch::FetcherConfig::default()).unwrap()
        });

        Self {
            config,
            fetcher,
            cache: HashMap::new(),
        }
    }

    /// Fetch and store content from a URL
    pub async fn fetch_and_store(&mut self, url: &str) -> Result<RagContent, RagError> {
        // Check cache first
        if let Some(cached) = self.cache.get(url) {
            return Ok(cached.clone());
        }

        // Fetch content
        let content = self.fetcher.fetch(url).await?;

        let rag_content = RagContent {
            id: uuid::Uuid::new_v4().to_string(),
            url: content.url.clone(),
            title: content.title.clone(),
            text: content.text.clone(),
            metadata: content.meta.clone(),
            fetched_at: Utc::now(),
            source_type: SourceType::WebFetch,
            relevance_score: 1.0,
        };

        // Cache it
        if self.cache.len() >= self.config.cache_size {
            // Remove oldest entry (simple eviction)
            if let Some(oldest_key) = self.cache.keys().next().cloned() {
                self.cache.remove(&oldest_key);
            }
        }
        self.cache.insert(url.to_string(), rag_content.clone());

        Ok(rag_content)
    }

    /// Fetch content using browser (for JavaScript-rendered pages)
    pub async fn fetch_with_browser(&mut self, url: &str) -> Result<RagContent, RagError> {
        let mut browser =
            soul_browser::BrowserController::new(soul_browser::BrowserConfig::default());

        browser
            .connect()
            .await
            .map_err(|e| RagError::Browser(e.to_string()))?;
        browser
            .navigate(url)
            .await
            .map_err(|e| RagError::Browser(e.to_string()))?;

        let text = browser
            .get_text()
            .await
            .map_err(|e| RagError::Browser(e.to_string()))?;
        let title = browser
            .get_page_state()
            .await
            .map(|s| s.title)
            .unwrap_or_default();

        let rag_content = RagContent {
            id: uuid::Uuid::new_v4().to_string(),
            url: url.to_string(),
            title,
            text,
            metadata: HashMap::new(),
            fetched_at: Utc::now(),
            source_type: SourceType::Browser,
            relevance_score: 1.0,
        };

        self.cache.insert(url.to_string(), rag_content.clone());

        Ok(rag_content)
    }

    /// Search cached content by relevance
    pub fn search(&self, query: &str) -> Vec<RagContent> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<RagContent> = self
            .cache
            .values()
            .map(|content| {
                let text_lower = content.text.to_lowercase();
                let title_lower = content.title.to_lowercase();

                // Simple TF scoring
                let mut score = 0.0f32;
                for word in &query_words {
                    if title_lower.contains(word) {
                        score += 3.0; // Title match bonus
                    }
                    let count = text_lower.matches(word).count() as f32;
                    score += count;
                }

                // Normalize by text length
                if !content.text.is_empty() {
                    score /= content.text.len() as f32 / 1000.0;
                }

                let mut c = content.clone();
                c.relevance_score = score.min(10.0);
                c
            })
            .filter(|content| content.relevance_score >= self.config.min_relevance)
            .collect();

        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        results.truncate(self.config.max_results);
        results
    }

    /// Fetch and search in one call
    pub async fn search_web(&mut self, query: &str) -> Result<Vec<RagContent>, RagError> {
        // Search cache first
        let cached = self.search(query);
        if !cached.is_empty() {
            return Ok(cached);
        }

        // If no cached results, try fetching the query as a URL
        if query.starts_with("http://") || query.starts_with("https://") {
            let content = self.fetch_and_store(query).await?;
            return Ok(vec![content]);
        }

        // Try browser fallback
        if self.config.use_browser_fallback {
            match self
                .fetch_with_browser(&format!("https://www.google.com/search?q={}", query))
                .await
            {
                Ok(content) => Ok(vec![content]),
                Err(_) => Err(RagError::NoResults),
            }
        } else {
            Err(RagError::NoResults)
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "cache_size": self.cache.len(),
            "max_cache_size": self.config.cache_size,
            "sources": {
                "web_fetch": self.cache.values().filter(|c| c.source_type == SourceType::WebFetch).count(),
                "browser": self.cache.values().filter(|c| c.source_type == SourceType::Browser).count(),
                "local": self.cache.values().filter(|c| c.source_type == SourceType::LocalMemory).count(),
            }
        })
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rag_config_default() {
        let config = RagConfig::default();
        assert_eq!(config.max_results, 10);
        assert_eq!(config.min_relevance, 0.5);
    }

    #[test]
    fn test_rag_store_search() {
        let config = RagConfig::default();
        let mut store = RagStore::new(config);

        // Add some test content
        store.cache.insert(
            "http://example.com".to_string(),
            RagContent {
                id: "1".into(),
                url: "http://example.com".into(),
                title: "Example Page".into(),
                text: "This is an example page about Rust programming".into(),
                metadata: HashMap::new(),
                fetched_at: Utc::now(),
                source_type: SourceType::WebFetch,
                relevance_score: 0.0,
            },
        );

        let results = store.search("Rust programming");
        assert!(!results.is_empty());
        assert_eq!(results[0].url, "http://example.com");
    }

    #[test]
    fn test_rag_store_stats() {
        let config = RagConfig::default();
        let store = RagStore::new(config);
        let stats = store.stats();
        assert_eq!(stats["cache_size"], 0);
    }

    #[test]
    fn test_rag_config_custom() {
        let config = RagConfig {
            max_results: 5,
            min_relevance: 0.8,
            fetch_timeout_ms: 15000,
            use_browser_fallback: false,
            cache_size: 50,
        };
        assert_eq!(config.max_results, 5);
        assert_eq!(config.min_relevance, 0.8);
        assert_eq!(config.cache_size, 50);
        assert!(!config.use_browser_fallback);
    }

    #[test]
    fn test_rag_store_search_no_cache() {
        let config = RagConfig::default();
        let store = RagStore::new(config);
        let results = store.search("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_rag_store_search_title_boosted() {
        let config = RagConfig::default();
        let mut store = RagStore::new(config);

        store.cache.insert(
            "http://rust.org".to_string(),
            RagContent {
                id: "1".into(),
                url: "http://rust.org".into(),
                title: "Rust Programming Language".into(),
                text: "A systems language focused on safety and performance.".into(),
                metadata: HashMap::new(),
                fetched_at: Utc::now(),
                source_type: SourceType::WebFetch,
                relevance_score: 0.0,
            },
        );

        let results = store.search("Rust");
        assert_eq!(results.len(), 1);
        assert!(results[0].relevance_score > 0.0);
    }

    #[test]
    fn test_rag_store_clear_cache() {
        let config = RagConfig::default();
        let mut store = RagStore::new(config);

        store.cache.insert(
            "http://example.com".to_string(),
            RagContent {
                id: "1".into(),
                url: "http://example.com".into(),
                title: "Test".into(),
                text: "some text".into(),
                metadata: HashMap::new(),
                fetched_at: Utc::now(),
                source_type: SourceType::WebFetch,
                relevance_score: 0.0,
            },
        );

        store.clear_cache();
        assert_eq!(store.cache.len(), 0);
    }

    #[test]
    fn test_rag_source_type_partial_eq() {
        assert_eq!(SourceType::WebFetch, SourceType::WebFetch);
        assert_eq!(SourceType::Browser, SourceType::Browser);
        assert_eq!(SourceType::LocalMemory, SourceType::LocalMemory);
        assert_eq!(SourceType::Document, SourceType::Document);
        assert_ne!(SourceType::WebFetch, SourceType::Browser);
    }

    #[test]
    fn test_rag_search_relevance_min_filter() {
        let config = RagConfig {
            min_relevance: 5.0,
            ..Default::default()
        };
        let mut store = RagStore::new(config);

        store.cache.insert(
            "http://example.com".to_string(),
            RagContent {
                id: "1".into(),
                url: "http://example.com".into(),
                title: "Title".into(),
                text: "just some unrelated content here".into(),
                metadata: HashMap::new(),
                fetched_at: Utc::now(),
                source_type: SourceType::WebFetch,
                relevance_score: 0.0,
            },
        );

        let results = store.search("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_rag_search_ranking_order() {
        let config = RagConfig {
            min_relevance: 0.0,
            ..Default::default()
        };
        let mut store = RagStore::new(config);

        store.cache.insert(
            "http://a.com".to_string(),
            RagContent {
                id: "1".into(),
                url: "http://a.com".into(),
                title: "Low relevance".into(),
                text: "some unrelated stuff".into(),
                metadata: HashMap::new(),
                fetched_at: Utc::now(),
                source_type: SourceType::WebFetch,
                relevance_score: 0.0,
            },
        );

        store.cache.insert(
            "http://b.com".to_string(),
            RagContent {
                id: "2".into(),
                url: "http://b.com".into(),
                title: "Rust Language".into(),
                text: "Rust is a systems programming language about Rust".into(),
                metadata: HashMap::new(),
                fetched_at: Utc::now(),
                source_type: SourceType::WebFetch,
                relevance_score: 0.0,
            },
        );

        let results = store.search("Rust");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "http://b.com");
        assert!(results[0].relevance_score >= results[1].relevance_score);
    }

    #[test]
    fn test_rag_search_max_results_limit() {
        let config = RagConfig {
            max_results: 1,
            min_relevance: 0.0,
            ..Default::default()
        };
        let mut store = RagStore::new(config);

        for i in 0..5 {
            store.cache.insert(
                format!("http://page{}.com", i),
                RagContent {
                    id: format!("{}", i),
                    url: format!("http://page{}.com", i),
                    title: "Page".into(),
                    text: format!("content about stuff {}", i),
                    metadata: HashMap::new(),
                    fetched_at: Utc::now(),
                    source_type: SourceType::WebFetch,
                    relevance_score: 0.0,
                },
            );
        }

        let results = store.search("about");
        assert_eq!(results.len(), 1);
    }
}
