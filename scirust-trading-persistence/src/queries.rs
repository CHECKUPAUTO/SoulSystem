//! Query API for persistence lookups.

use rusqlite::Connection;
use scirust_trading_core::codified::EnrichmentLevel;

/// Stats response from `decision_stats`.
#[derive(Debug, Default, serde::Serialize)]
pub struct DecisionStatsResult {
    pub total: u64,
    pub no_trade: u64,
    pub hold: u64,
    pub open: u64,
    pub close: u64,
}

/// Simple query interface for historical data.
pub struct QueryApi {
    // Conservé pour l'implémentation réelle des requêtes ; les méthodes
    // actuelles sont des stubs qui renvoient des données vides.
    #[allow(dead_code)]
    conn: std::sync::Arc<std::sync::Mutex<Connection>>,
}

/// Aggregated statistics container returned by `aggregate_stats`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AggregatedStats {
    pub n: u64,
    pub overall: crate::GroupStats,
    pub by_gate: std::collections::HashMap<String, crate::GroupStats>,
    pub by_bias: std::collections::HashMap<String, crate::GroupStats>,
    pub by_symbol: std::collections::HashMap<String, crate::GroupStats>,
}

impl QueryApi {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
        }
    }

    /// Open in-memory SQLite database.
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Ok(Self::new(conn))
    }

    /// Fetch recent events, optionally filtered by category and minimum enrichment.
    pub async fn recent_events(
        &self,
        _limit: u32,
        _category: Option<scirust_trading_core::Category>,
        _min_enrich: Option<EnrichmentLevel>,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!([]))
    }

    /// Fetch recent decisions, optionally filtered by action.
    pub async fn recent_decisions(
        &self,
        _limit: u32,
        _action: Option<String>,
    ) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    /// Fetch decision stats in a time range.
    pub async fn decision_stats(
        &self,
        _from: chrono::DateTime<chrono::Utc>,
        _to: chrono::DateTime<chrono::Utc>,
    ) -> Result<DecisionStatsResult, String> {
        Ok(DecisionStatsResult::default())
    }

    /// Compute outcomes in a time range with given config.
    pub async fn compute_outcomes_in_range(
        &self,
        _from: chrono::DateTime<chrono::Utc>,
        _to: chrono::DateTime<chrono::Utc>,
        _config: crate::OutcomeConfig,
    ) -> Result<AggregatedStats, String> {
        Ok(AggregatedStats::default())
    }

    /// Aggregate stats from a stats group.
    pub fn aggregate_stats(&self, _stats: &AggregatedStats) -> AggregatedStats {
        AggregatedStats::default()
    }

    /// Look up historical response for a set of tags.
    pub async fn historical_response_for_tags(
        &self,
        _tags: &[String],
        _asset_symbol_canonical: &str,
        _exchange: &str,
    ) -> Result<Option<crate::MarketReaction>, String> {
        Ok(None)
    }
}
