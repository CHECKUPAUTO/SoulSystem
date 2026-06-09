//! SciRust Trading Core — Types, market state, events, and reactions.

use serde::{Deserialize, Serialize};

// ── Supporting types (must come first for derive macros) ────────────────

/// Supported exchanges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    Binance,
    Coinbase,
    Kraken,
    Bybit,
    Okx,
    Other,
}

/// Trading symbol (e.g. BTC/USDT).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub base: String,
    pub quote: String,
}

impl Symbol {
    pub fn new(base: &str, quote: &str) -> Self {
        Self {
            base: base.to_string(),
            quote: quote.to_string(),
        }
    }

    pub fn pair(&self) -> String {
        format!("{}/{}", self.base, self.quote)
    }
}

/// Order side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// Polarity (sentiment direction) with value in [-1, 1].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Polarity(f64);

impl Polarity {
    pub fn new(v: f64) -> Self {
        Self(v.clamp(-1.0, 1.0))
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

/// Target asset for an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Symbol(Symbol),
    All,
}

/// Source identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// When an event was observed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EventTiming {
    Observed(chrono::DateTime<chrono::Utc>),
    Predicted(chrono::DateTime<chrono::Utc>),
}

/// Source reliability score [0, 1].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Reliability(f64);

impl Reliability {
    pub fn new(v: f64) -> Self {
        Self(v.clamp(0.0, 1.0))
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

/// Category of a trading event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

// ── Types module ────────────────────────────────────────────────────────

pub mod types {
    use super::*;

    /// A single OHLCV bar.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Bar {
        pub exchange: Exchange,
        pub symbol: Symbol,
        pub timestamp: chrono::DateTime<chrono::Utc>,
        pub open: f64,
        pub high: f64,
        pub low: f64,
        pub close: f64,
        pub volume: f64,
    }

    /// A limit or market order.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Order {
        pub id: uuid::Uuid,
        pub symbol: Symbol,
        pub side: Side,
        pub price: f64,
        pub quantity: f64,
        pub timestamp: chrono::DateTime<chrono::Utc>,
    }

    /// A filled trade.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Trade {
        pub id: uuid::Uuid,
        pub symbol: Symbol,
        pub side: Side,
        pub price: f64,
        pub quantity: f64,
        pub timestamp: chrono::DateTime<chrono::Utc>,
        pub is_maker: bool,
    }
}

// ── Market module ───────────────────────────────────────────────────────

pub mod market {
    use super::*;

    /// Real-time market microstructure state.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MarketState {
        pub exchange: Exchange,
        pub symbol: Symbol,
        pub timestamp: chrono::DateTime<chrono::Utc>,
        pub mid: f64,
        pub microprice: f64,
        pub spread_bps: f64,
        pub imbalance_5: f64,
        pub imbalance_20: f64,
        pub realized_vol_pct: f64,
        pub volume_1min: f64,
        pub flow_imbalance_1min: f64,
        pub trade_count_1min: u64,
    }

    impl MarketState {
        pub fn exchange_str(&self) -> &'static str {
            match self.exchange {
                Exchange::Binance => "Binance",
                Exchange::Coinbase => "Coinbase",
                Exchange::Kraken => "Kraken",
                Exchange::Bybit => "Bybit",
                Exchange::Okx => "Okx",
                Exchange::Other => "Other",
            }
        }
    }
}

// ── Codified module ─────────────────────────────────────────────────────

pub mod codified {
    use super::*;

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
        pub historical_response: Option<MarketReaction>,
        pub explanation: Option<String>,
        pub detected_at: chrono::DateTime<chrono::Utc>,
        pub semantic_summary: Option<String>,
        pub polarity: Option<Polarity>,
        pub magnitude: Option<f64>,
        pub semantic_confidence: Option<f64>,
        pub targets: Vec<Target>,
        pub source_reliability: Reliability,
        pub category: Category,
        pub timing: EventTiming,
    }

    impl CodifiedEvent {
        pub fn builder<S: Into<String>>(source: S, text: S) -> CodifiedEventBuilder {
            CodifiedEventBuilder::new(source, text)
        }

        /// Compute weighted score: polarity × magnitude × confidence × reliability × decay.
        pub fn weighted_score(&self, now: chrono::DateTime<chrono::Utc>) -> f64 {
            let pol = self.polarity.map(|p| p.value()).unwrap_or(0.0);
            let mag = self.magnitude.unwrap_or(0.0);
            let conf = self.semantic_confidence.unwrap_or(0.0);
            let rel = self.source_reliability.value();
            let age_hours = (now - self.detected_at).num_minutes() as f64 / 60.0;
            let decay = (-age_hours / 24.0).exp();
            pol * mag * conf * rel * decay
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
        category: Category,
        timing: EventTiming,
        reliability: f64,
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
                category: Category::Other,
                timing: EventTiming::Observed(chrono::Utc::now()),
                reliability: 0.5,
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

        pub fn category(mut self, cat: Category) -> Self {
            self.category = cat;
            self
        }

        pub fn timing(mut self, t: EventTiming) -> Self {
            self.timing = t;
            self
        }

        pub fn reliability(mut self, r: f64) -> Self {
            self.reliability = r;
            self
        }

        pub fn build(self) -> CodifiedEvent {
            let now = chrono::Utc::now();
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
                detected_at: now,
                semantic_summary: None,
                polarity: None,
                magnitude: None,
                semantic_confidence: None,
                targets: Vec::new(),
                source_reliability: Reliability::new(self.reliability),
                category: self.category,
                timing: self.timing,
            }
        }
    }
}

// ── Reaction module ─────────────────────────────────────────────────────

pub mod reaction {
    use super::*;

    /// Market reaction data for a given event.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct MarketReaction {
        pub direction: i8,
        pub magnitude: f64,
        pub confidence: f64,
        pub timestamp_ms: i64,
        pub n_samples: usize,
        pub delta_5min_bps: f64,
        pub delta_15min_bps: f64,
        pub delta_60min_bps: f64,
        pub delta_60min_std_bps: f64,
        pub volume_spike_ratio: f64,
    }

    impl MarketReaction {
        /// Is this reaction statistically significant?
        pub fn is_significant(&self) -> bool {
            self.n_samples >= 3 && self.delta_60min_bps.abs() > self.delta_60min_std_bps * 1.5
        }
    }
}

// ── Bus module ──────────────────────────────────────────────────────────

pub mod bus;

// ── Re-exports ──────────────────────────────────────────────────────────

pub use bus::EventBus;
pub use codified::{CodifiedEvent, EnrichmentLevel};
pub use market::MarketState;
pub use reaction::MarketReaction;
pub use types::{Bar, Order, Trade};
