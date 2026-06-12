pub mod embeddings;
pub mod queries;
pub mod schema;
mod stub_news;

pub use embeddings::SqliteEmbeddingStore;
pub use queries::QueryApi;
pub use schema::GroupStats;
pub use stub_news::*;

/// Outcome configuration for performance tracking.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OutcomeConfig {
    pub pnl: f64,
    pub sharpe: Option<f64>,
    pub drawdown: Option<f64>,
    pub win_rate: Option<f64>,
}

impl OutcomeConfig {
    /// Default typical outcome config.
    pub fn typical() -> Self {
        Self::default()
    }
}

/// Result type for persistence operations.
pub type PersistenceResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Market reaction data — placeholder for the missing trading-news crate type.
#[derive(Debug, Clone, Default)]
pub struct MarketReaction {
    pub direction: i8,
    pub magnitude: f64,
    pub confidence: f64,
}
