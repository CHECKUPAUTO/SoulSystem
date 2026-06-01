pub mod contextual;
pub use contextual::{
    cosine_similarity, ContextualConfig, ContextualEnricher, EmbeddingStore, HistoricalProvider,
    TribeClient, TribeConfig,
};

/// Error type for news/trading operations.
#[derive(Debug)]
pub enum NewsError {
    Http(String),
    Parse(String),
    NotFound(String),
    Embedding(String),
}

impl std::fmt::Display for NewsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewsError::Http(msg) => write!(f, "http: {msg}"),
            NewsError::Parse(msg) => write!(f, "parse: {msg}"),
            NewsError::NotFound(msg) => write!(f, "not found: {msg}"),
            NewsError::Embedding(msg) => write!(f, "embedding: {msg}"),
        }
    }
}

impl std::error::Error for NewsError {}

/// Convenience result type for news operations.
pub type NewsResult<T> = Result<T, NewsError>;

/// A stored embedding entry for similarity search.
#[derive(Debug, Clone)]
pub struct StoredEmbedding {
    pub event_id: uuid::Uuid,
    pub vector: Vec<f32>,
    pub metadata: String,
}
