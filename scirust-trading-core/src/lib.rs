//! SciRust Trading Core

pub mod types {
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct Bar;
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct Order;
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct Trade;
}
pub mod market {
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct MarketState;
}
pub mod codified {
    use serde::{Deserialize, Serialize};

    /// Enrichment level for a codified event.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum EnrichmentLevel {
        Raw,
        Structural,
        Semantic,
        Contextual,
        Fused,
    }

    /// A codified event in the trading pipeline.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CodifiedEvent {
        pub id: uuid::Uuid,
        pub title: String,
        pub raw_text: String,
        pub body: String,
        pub url: Option<String>,
        pub enrichment: EnrichmentLevel,
        pub tags: Vec<String>,
        pub timestamp_ms: i64,
        pub source: String,
        pub summary: Option<String>,
        pub relevance_score: f64,
        pub nearest_neighbors: Vec<(uuid::Uuid, f64)>,
        pub historical_response: Option<super::reaction::MarketReaction>,
        pub explanation: Option<String>,
    }

    impl CodifiedEvent {
        pub fn builder<S: Into<String>>(source: S, text: S) -> CodifiedEventBuilder {
            CodifiedEventBuilder::new(source, text)
        }
    }

    /// Builder for CodifiedEvent.
    pub struct CodifiedEventBuilder {
        title: String,
        raw_text: String,
        source: String,
        enrichment: EnrichmentLevel,
        tags: Vec<String>,
        timestamp_ms: i64,
    }

    impl CodifiedEventBuilder {
        pub fn new<S: Into<String>>(source: S, text: S) -> Self {
            Self {
                title: String::new(),
                raw_text: text.into(),
                source: source.into(),
                enrichment: EnrichmentLevel::Raw,
                tags: Vec::new(),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            }
        }

        pub fn title(mut self, title: &str) -> Self {
            self.title = title.to_string();
            self
        }

        pub fn enrichment(mut self, level: EnrichmentLevel) -> Self {
            self.enrichment = level;
            self
        }

        pub fn tag(mut self, tag: &str) -> Self {
            self.tags.push(tag.to_string());
            self
        }

        pub fn timestamp(mut self, ms: i64) -> Self {
            self.timestamp_ms = ms;
            self
        }

        pub fn build(self) -> CodifiedEvent {
            CodifiedEvent {
                id: uuid::Uuid::new_v4(),
                title: self.title,
                raw_text: self.raw_text.clone(),
                body: self.raw_text,
                url: None,
                enrichment: self.enrichment,
                tags: self.tags,
                timestamp_ms: self.timestamp_ms,
                source: self.source,
                summary: None,
                relevance_score: 0.0,
                nearest_neighbors: Vec::new(),
                historical_response: None,
                explanation: None,
            }
        }
    }
}
pub mod reaction {
    /// Market reaction data for a given event.
    #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
    pub struct MarketReaction {
        pub direction: i8,
        pub magnitude: f64,
        pub confidence: f64,
        pub timestamp_ms: i64,
    }
}
pub mod bus;

// Re-exports for convenience
pub use codified::{CodifiedEvent, EnrichmentLevel};
pub use market::MarketState;
pub use reaction::MarketReaction;
pub use types::{Bar, Order, Trade};

/// Category of a trading event (macro, micro, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Category {
    Macro,
    Micro,
    Sentiment,
    Alpha,
    Technical,
    Fundamental,
    News,
    Regulatory,
    ExchangeEvent,
    OnChain,
    Narrative,
    Liquidation,
    Funding,
    Other,
}
