use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RagError {
    #[error("Fetch error: {0}")]
    Fetch(String),
    #[error("Browser error: {0}")]
    Browser(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("No results found")]
    NoResults,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    WebFetch,
    Browser,
    LocalMemory,
    Document,
}

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

    pub async fn fetch_and_store(&mut self, url: &str) -> Result<RagContent, RagError> {
        if let Some(cached) = self.cache.get(url) {
            return Ok(cached.clone());
        }

        let content = self
            .fetcher
            .fetch(url)
            .await
            .map_err(|e| RagError::Fetch(format!("{:?}", e)))?;

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

        if self.cache.len() >= self.config.cache_size {
            if let Some(oldest_key) = self.cache.keys().next().cloned() {
                self.cache.remove(&oldest_key);
            }
        }
        self.cache.insert(url.to_string(), rag_content.clone());
        Ok(rag_content)
    }

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

    pub fn search(&self, query: &str) -> Vec<RagContent> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<RagContent> = self
            .cache
            .values()
            .map(|content| {
                let text_lower = content.text.to_lowercase();
                let title_lower = content.title.to_lowercase();
                let mut score = 0.0f32;
                for word in &query_words {
                    if title_lower.contains(word) {
                        score += 3.0;
                    }
                    let count = text_lower.matches(word).count() as f32;
                    score += count;
                }
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

    pub async fn search_web(&mut self, query: &str) -> Result<Vec<RagContent>, RagError> {
        let cached = self.search(query);
        if !cached.is_empty() {
            return Ok(cached);
        }
        if query.starts_with("http://") || query.starts_with("https://") {
            let content = self.fetch_and_store(query).await?;
            return Ok(vec![content]);
        }
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
    }

    #[test]
    fn test_rag_store_search() {
        let mut store = RagStore::new(RagConfig::default());
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
    }

    #[test]
    fn test_rag_store_clear_cache() {
        let mut store = RagStore::new(RagConfig::default());
        store.cache.insert(
            "http://x.com".to_string(),
            RagContent {
                id: "1".into(),
                url: "http://x.com".into(),
                title: "Test".into(),
                text: "text".into(),
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
    fn test_rag_source_type() {
        assert_eq!(SourceType::WebFetch, SourceType::WebFetch);
        assert_ne!(SourceType::WebFetch, SourceType::Browser);
    }
}
