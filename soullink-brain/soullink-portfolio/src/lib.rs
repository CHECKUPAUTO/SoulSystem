//! SoulLink Portfolio — Autonomous crypto portfolio manager.
//!
//! Architecture:
//! - Binance WebSocket → real-time market data
//! - HNN crypto organ (port 9013) → buy/sell signals
//! - Paper trading engine → simulated portfolio tracking
//! - PostgreSQL/Sled → trade history + P&L
//!
//! Safety: Paper trading by default. Real trading requires explicit
//! TRADING_MODE=live env var + max_position_usd + daily_loss_limit.

pub mod binance;
pub mod engine;
pub mod portfolio;
pub mod risk;
pub mod types;

pub use engine::PortfolioEngine;
pub use portfolio::Portfolio;
pub use risk::RiskManager;