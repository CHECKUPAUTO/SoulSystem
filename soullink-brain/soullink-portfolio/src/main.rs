//! SoulLink Trader — autonomous crypto portfolio manager.
//!
//! PAPER TRADING by default. Set TRADING_MODE=live for real (requires API keys).
//! NEVER runs in live mode without explicit consent.

use soullink_portfolio::PortfolioEngine;
use soullink_portfolio::types::RiskLimits;
use rust_decimal_macros::dec;

#[tokio::main(worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // SAFETY: Paper trading only. No API keys loaded.
    let mode = std::env::var("TRADING_MODE").unwrap_or_default();
    if mode == "live" {
        tracing::error!("🚨 TRADING_MODE=live is NOT IMPLEMENTED. Use paper trading.");
        std::process::exit(1);
    }

    tracing::info!("🦞 SoulLink Trader V13.5 — PAPER TRADING MODE");
    tracing::info!("⚠️  No real money. Simulated portfolio only.");

    let starting_capital = dec!(1000); // $1000 paper
    let limits = RiskLimits::conservative();
    let organ_url = "http://127.0.0.1:9013".to_string();
    let symbols = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string(), "SOLUSDT".to_string()];

    let mut engine = PortfolioEngine::new(starting_capital, limits, &organ_url, symbols);

    if let Err(e) = engine.run().await {
        tracing::error!("Engine error: {e}");
    }
}