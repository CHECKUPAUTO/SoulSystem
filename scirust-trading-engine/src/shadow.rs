//! Shadow evaluator module.
//! Maintains a sliding window of active events.

use scirust_trading_core::bus::EventBus;
use std::sync::Arc;

/// Configuration for the shadow evaluator.
#[derive(Debug, Clone)]
pub struct ShadowConfig {
    pub decisions_channel_cap: usize,
    pub virtual_equity_quote: f64,
    pub track_virtual_positions: bool,
    pub atr_period: Option<usize>,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            decisions_channel_cap: 512,
            virtual_equity_quote: 10_000.0,
            track_virtual_positions: true,
            atr_period: Some(14),
        }
    }
}

/// Snapshot of the virtual portfolio state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PortfolioSnapshot {
    pub equity_quote: f64,
    pub realized_pnl: f64,
    total_unrealized: f64,
    total_exposure_value: f64,
    position_count_value: u32,
}

impl PortfolioSnapshot {
    pub fn total_unrealized_pnl(&self) -> f64 {
        self.total_unrealized
    }
    pub fn total_exposure(&self) -> f64 {
        self.total_exposure_value
    }
    pub fn position_count(&self) -> usize {
        self.position_count_value as usize
    }
}

/// Shadow evaluator — processes events and market state.
pub struct ShadowEvaluator {
    _config: ShadowConfig,
    decisions_tx: tokio::sync::broadcast::Sender<String>,
}

impl ShadowEvaluator {
    pub fn new(config: ShadowConfig) -> Self {
        let (decisions_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            _config: config,
            decisions_tx,
        }
    }

    pub fn spawn(self: Arc<Self>, _bus: &EventBus) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // Shadow loop — placeholder for event processing
        })
    }

    /// Subscribe to shadow decisions stream.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.decisions_tx.subscribe()
    }

    /// Get a snapshot of the virtual portfolio.
    pub async fn portfolio_snapshot(&self) -> PortfolioSnapshot {
        PortfolioSnapshot {
            equity_quote: self._config.virtual_equity_quote,
            realized_pnl: 0.0,
            total_unrealized: 0.0,
            total_exposure_value: 0.0,
            position_count_value: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_config_default() {
        let config = ShadowConfig::default();
        assert_eq!(config.virtual_equity_quote, 10_000.0);
        assert!(config.track_virtual_positions);
        assert_eq!(config.atr_period, Some(14));
    }

    #[test]
    fn test_portfolio_snapshot() {
        let snapshot = PortfolioSnapshot {
            equity_quote: 10_000.0,
            realized_pnl: 500.0,
            total_unrealized: 200.0,
            total_exposure_value: 5_000.0,
            position_count_value: 3,
        };
        assert_eq!(snapshot.equity_quote, 10_000.0);
        assert_eq!(snapshot.position_count(), 3);
        assert_eq!(snapshot.total_exposure(), 5_000.0);
    }

    #[test]
    fn test_shadow_evaluator_new() {
        let evaluator = ShadowEvaluator::new(ShadowConfig::default());
        let _rx = evaluator.subscribe();
    }

    #[tokio::test]
    async fn test_portfolio_snapshot_empty() {
        let evaluator = ShadowEvaluator::new(ShadowConfig::default());
        let snapshot = evaluator.portfolio_snapshot().await;
        assert_eq!(snapshot.equity_quote, 10_000.0);
        assert_eq!(snapshot.position_count(), 0);
    }
}
