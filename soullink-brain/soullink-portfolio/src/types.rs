//! Core types for the portfolio engine.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Trading pair (e.g. BTCUSDT, ETHUSDT).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Symbol(String);

impl Symbol {
    pub fn new(s: &str) -> Self {
        Self(s.to_uppercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Base asset (BTC in BTCUSDT).
    pub fn base(&self) -> &str {
        // Most Binance pairs end in USDT, BUSD, BTC, ETH, BNB
        for quote in ["USDT", "BUSD", "BTC", "ETH", "BNB", "FDUSD"] {
            if self.0.ends_with(quote) {
                return &self.0[..self.0.len() - quote.len()];
            }
        }
        &self.0
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Side of a trade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

/// A market tick from Binance WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: Symbol,
    pub price: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    /// Price change % in last 24h (from 24hr ticker).
    pub change_24h_pct: Decimal,
}

/// Signal from the HNN crypto organ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnnSignal {
    pub attractor: String,
    pub turbulence: f64,
    pub momentum: f64,
    pub spike_rate: f64,
    /// Derived action: positive = buy strength, negative = sell strength, 0 = hold.
    pub action: f64,
}

impl HnnSignal {
    /// Convert HNN state to a trading signal.
    /// - StrangeAttractor + high momentum → strong signal
    /// - DeepBasin → hold
    /// - High turbulence → reduce position
    pub fn from_organ_stats(stats: &serde_json::Value) -> Self {
        let turbulence = stats["turbulence"]["value"].as_f64().unwrap_or(0.0);
        let attractor = stats["turbulence"]["attractor"]
            .as_str()
            .unwrap_or("Transient")
            .to_string();
        let momentum = stats["avg_momentum"].as_f64().unwrap_or(0.0);
        let spike_rate = stats["spike_rate"].as_f64().unwrap_or(0.0);

        // Action calculation:
        // - Momentum direction (positive = bullish, negative = bearish)
        // - Attractor modulation (StrangeAttractor amplifies, DeepBasin suppresses)
        // - Turbulence discount (high turbulence = uncertain, reduce confidence)
        let attractor_mod = match attractor.as_str() {
            "StrangeAttractor" => 1.5,  // amplify
            "StableOrbit" => 1.0,       // neutral
            "DeepBasin" => 0.2,         // suppress
            "Transient" => 0.6,         // cautious
            _ => 0.5,
        };

        let turbulence_discount = 1.0 - (turbulence * 0.5); // high turbulence = lower confidence

        let action = momentum * attractor_mod * turbulence_discount;

        HnnSignal {
            attractor,
            turbulence,
            momentum,
            spike_rate,
            action,
        }
    }
}

/// A trade execution (paper or real).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: u64,
    pub symbol: Symbol,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
    pub cost: Decimal,  // price * quantity
    pub fee: Decimal,    // 0.1% Binance default
    pub timestamp: DateTime<Utc>,
    pub signal: HnnSignal,
    pub is_paper: bool,
}

/// Portfolio position in a single asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: Symbol,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub unrealized_pnl_pct: Decimal,
}

impl Position {
    pub fn is_empty(&self) -> bool {
        self.quantity == Decimal::ZERO
    }
}

/// Portfolio snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub total_value_usd: Decimal,
    pub cash_usd: Decimal,
    pub positions: Vec<Position>,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub total_trades: u64,
    pub win_rate: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// Risk limits — hard caps that cannot be exceeded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    /// Maximum position size in USD per trade.
    pub max_position_usd: Decimal,
    /// Maximum total portfolio exposure (sum of all positions).
    pub max_total_exposure_usd: Decimal,
    /// Daily loss limit — stop trading if exceeded.
    pub daily_loss_limit_usd: Decimal,
    /// Maximum number of open positions.
    pub max_open_positions: usize,
    /// Minimum confidence (|action|) to trigger a trade.
    pub min_signal_confidence: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_usd: dec!(100),        // $100 max per position
            max_total_exposure_usd: dec!(500),   // $500 total
            daily_loss_limit_usd: dec!(50),      // $50 daily loss cap
            max_open_positions: 5,
            min_signal_confidence: 0.3,          // 30% confidence minimum
        }
    }
}

/// Conservative limits for paper trading start.
impl RiskLimits {
    pub fn conservative() -> Self {
        Self {
            max_position_usd: dec!(50),
            max_total_exposure_usd: dec!(200),
            daily_loss_limit_usd: dec!(20),
            max_open_positions: 3,
            min_signal_confidence: 0.4,
        }
    }
}