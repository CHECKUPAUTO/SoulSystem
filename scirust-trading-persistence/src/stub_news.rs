//! Stub types replacing the missing `scirust_trading_news` crate.
//! Provides just enough for SqliteEmbeddingStore to compile.

use std::fmt;
use uuid::Uuid;

/// Cosine similarity between two f32 slices.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    let na: f64 = a.iter().map(|x| (*x as f64) * (*x as f64)).sum();
    let nb: f64 = b.iter().map(|x| (*x as f64) * (*x as f64)).sum();
    let denom = (na * nb).sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// News crate error type.
#[derive(Debug)]
pub enum NewsError {
    Http(String),
    Parse(String),
    NotFound(String),
}

impl fmt::Display for NewsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NewsError::Http(msg) => write!(f, "http: {msg}"),
            NewsError::Parse(msg) => write!(f, "parse: {msg}"),
            NewsError::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for NewsError {}

/// News crate result alias.
pub type NewsResult<T> = Result<T, NewsError>;

/// EmbeddingStore trait — store and query vector embeddings.
#[async_trait::async_trait]
pub trait EmbeddingStore: Send + Sync {
    /// Store an embedding for a given event.
    async fn put(&self, event_id: Uuid, embedding: Vec<f32>) -> NewsResult<()>;

    /// Find the k nearest neighbors by cosine similarity.
    async fn find_nearest(
        &self,
        query: &[f32],
        k: usize,
        exclude: Option<Uuid>,
    ) -> NewsResult<Vec<(Uuid, f64)>>;
}

/// HistoricalProvider trait — look up past market reactions.
#[async_trait::async_trait]
pub trait HistoricalProvider: Send + Sync {
    async fn lookup_response(
        &self,
        tags: &[String],
        asset_symbol_canonical: &str,
        exchange: &str,
    ) -> NewsResult<Option<crate::MarketReaction>>;
}
