//! Portfolio engine — orchestrates market data, HNN signals, and trading.

use crate::binance;
use crate::portfolio::Portfolio;
use crate::risk::{RiskManager, RiskVerdict};
use crate::types::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// The trading engine — connects everything together.
pub struct PortfolioEngine {
    portfolio: Portfolio,
    risk: RiskManager,
    client: reqwest::Client,
    organ_url: String,
    symbols: Vec<String>,
    /// Starting capital for P&L tracking.
    starting_capital: Decimal,
}

impl PortfolioEngine {
    /// Create a new paper trading engine.
    pub fn new(
        starting_cash: Decimal,
        limits: RiskLimits,
        organ_url: &str,
        symbols: Vec<String>,
    ) -> Self {
        let starting_capital = starting_cash;
        Self {
            portfolio: Portfolio::new(starting_cash),
            risk: RiskManager::new(limits),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("reqwest"),
            organ_url: organ_url.to_string(),
            symbols,
            starting_capital,
        }
    }

    /// Run the main trading loop.
    pub async fn run(&mut self) -> Result<(), String> {
        info!("🦞 Portfolio Engine starting — PAPER TRADING");
        info!(
            "Capital: ${} | Symbols: {:?}",
            self.portfolio.cash(),
            self.symbols
        );

        let (tick_tx, mut tick_rx) = mpsc::channel::<Tick>(1024);

        // Start Binance WebSocket in background
        let symbols = self.symbols.clone();
        tokio::spawn(async move {
            if let Err(e) = binance::stream_ticks(&symbols, tick_tx).await {
                error!("Binance WS error: {e}");
            }
        });

        // Fetch initial 24h tickers
        let changes = binance::fetch_24h_tickers(&self.client, &self.symbols).await?;
        info!("24h changes: {:?}", changes);

        // Main loop
        let mut prices: HashMap<String, Decimal> = HashMap::new();
        let mut tick_count: u64 = 0;
        let mut last_signal_check = std::time::Instant::now();

        while let Some(tick) = tick_rx.recv().await {
            tick_count += 1;
            prices.insert(tick.symbol.as_str().to_string(), tick.price);

            // Update portfolio prices
            self.portfolio.update_prices(&prices);

            // Check HNN signal every 30 seconds
            if last_signal_check.elapsed() > std::time::Duration::from_secs(30) {
                last_signal_check = std::time::Instant::now();

                if let Ok(signal) = self.fetch_hnn_signal().await {
                    self.evaluate_and_trade(&signal, &prices).await;
                }
            }

            // Log status every 1000 ticks
            if tick_count % 1000 == 0 {
                let snap = self.portfolio.snapshot(&prices);
                info!(
                    "📊 Portfolio: ${} | P&L: ${} | Trades: {} | Win: {:.1}%",
                    snap.total_value_usd,
                    snap.realized_pnl + snap.unrealized_pnl,
                    snap.total_trades,
                    snap.win_rate * dec!(100),
                );
            }
        }

        Ok(())
    }

    /// Fetch HNN signal from the crypto organ.
    async fn fetch_hnn_signal(&self) -> Result<HnnSignal, String> {
        let url = format!("{}/api/stats", self.organ_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("organ fetch: {e}"))?;

        let stats: serde_json::Value =
            resp.json().await.map_err(|e| format!("organ parse: {e}"))?;

        Ok(HnnSignal::from_organ_stats(&stats))
    }

    /// Evaluate signal and execute trades if appropriate.
    async fn evaluate_and_trade(&mut self, signal: &HnnSignal, prices: &HashMap<String, Decimal>) {
        info!(
            "🧠 HNN: attractor={} turbulence={:.3} action={:.3}",
            signal.attractor, signal.turbulence, signal.action
        );

        // Only trade the first symbol for now (expand later)
        if let Some(symbol_str) = self.symbols.first() {
            let symbol = Symbol::new(symbol_str);
            let price = match prices.get(symbol_str) {
                Some(p) => *p,
                None => {
                    warn!("No price for {}", symbol);
                    return;
                }
            };

            // Determine trade side from signal
            let side = if signal.action > 0.0 {
                // Bullish — check if we have position to add to
                if self.portfolio.get_position(&symbol).is_some() {
                    return; // already in position
                }
                Side::Buy
            } else if signal.action < 0.0 {
                // Bearish — check if we have position to close
                if self.portfolio.get_position(&symbol).is_none() {
                    return; // no position to close
                }
                Side::Sell
            } else {
                return; // hold
            };

            // Calculate quantity based on position limit
            let max_usd = self.risk.limits.max_position_usd;
            let quantity = max_usd / price;

            // Risk check
            let quantity =
                match self
                    .risk
                    .check(&self.portfolio, &symbol, side, price, quantity, signal)
                {
                    RiskVerdict::Allow => quantity,
                    RiskVerdict::Deny(reason) => {
                        info!("🛑 Trade blocked: {}", reason);
                        return;
                    }
                    RiskVerdict::ReducePosition {
                        max_quantity,
                        reason,
                    } => {
                        info!("⚠️ Position reduced: {}", reason);
                        max_quantity
                    }
                };

            // Execute
            match self.portfolio.execute_trade(
                symbol.clone(),
                side,
                price,
                quantity,
                signal.clone(),
            ) {
                Ok(trade) => {
                    info!(
                        "✅ {} {} {} @ ${} | cost=${} fee=${}",
                        trade.side,
                        trade.quantity,
                        trade.symbol,
                        trade.price,
                        trade.cost,
                        trade.fee
                    );
                }
                Err(e) => warn!("Trade failed: {e}"),
            }
        }
    }

    /// Get current portfolio snapshot.
    pub fn snapshot(&self, prices: &HashMap<String, Decimal>) -> PortfolioSnapshot {
        self.portfolio.snapshot(prices)
    }
}
