//! Portfolio state management — tracks positions, P&L, and trade history.

use crate::types::*;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// The portfolio — tracks all positions and P&L.
#[derive(Debug, Clone)]
pub struct Portfolio {
    /// Available cash (paper USD).
    cash: Decimal,
    /// Open positions by symbol.
    positions: HashMap<String, Position>,
    /// Completed trades.
    trade_history: Vec<Trade>,
    /// Cumulative realized P&L.
    realized_pnl: Decimal,
    /// Next trade ID.
    next_trade_id: u64,
}

impl Portfolio {
    /// Create a new paper portfolio with starting cash.
    pub fn new(starting_cash: Decimal) -> Self {
        Self {
            cash: starting_cash,
            positions: HashMap::new(),
            trade_history: Vec::new(),
            realized_pnl: Decimal::ZERO,
            next_trade_id: 1,
        }
    }

    /// Get current cash balance.
    pub fn cash(&self) -> Decimal {
        self.cash
    }

    /// Get a position by symbol.
    pub fn get_position(&self, symbol: &Symbol) -> Option<&Position> {
        self.positions.get(symbol.as_str())
    }

    /// Number of open positions.
    pub fn open_position_count(&self) -> usize {
        self.positions.values().filter(|p| !p.is_empty()).count()
    }

    /// Total realized P&L.
    pub fn realized_pnl(&self) -> Decimal {
        self.realized_pnl
    }

    /// Total trade count.
    pub fn trade_count(&self) -> u64 {
        self.trade_history.len() as u64
    }

    /// Win rate (trades with positive P&L / total closed trades).
    pub fn win_rate(&self) -> Decimal {
        if self.trade_history.is_empty() {
            return Decimal::ZERO;
        }
        let wins = self
            .trade_history
            .iter()
            .filter(|t| t.side == Side::Sell) // sells realize P&L
            .count();
        let total_sells = self
            .trade_history
            .iter()
            .filter(|t| t.side == Side::Sell)
            .count();
        if total_sells == 0 {
            return Decimal::ZERO;
        }
        Decimal::from(wins) / Decimal::from(total_sells)
    }

    /// Execute a trade (paper — no real exchange interaction).
    pub fn execute_trade(
        &mut self,
        symbol: Symbol,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        signal: HnnSignal,
    ) -> Result<Trade, String> {
        let fee_rate = dec!(0.001); // 0.1% Binance default
        let cost = price * quantity;

        match side {
            Side::Buy => {
                let fee = cost * fee_rate;
                let total_cost = cost + fee;

                if total_cost > self.cash {
                    return Err(format!(
                        "insufficient cash: need ${} have ${}",
                        total_cost, self.cash
                    ));
                }

                self.cash -= total_cost;

                // Update or create position
                let pos = self
                    .positions
                    .entry(symbol.as_str().to_string())
                    .or_insert(Position {
                        symbol: symbol.clone(),
                        quantity: Decimal::ZERO,
                        avg_entry_price: Decimal::ZERO,
                        current_price: price,
                        unrealized_pnl: Decimal::ZERO,
                        unrealized_pnl_pct: Decimal::ZERO,
                    });

                // Weighted average entry price
                let old_total = pos.quantity * pos.avg_entry_price;
                let new_total = quantity * price;
                pos.avg_entry_price = if pos.quantity + quantity > Decimal::ZERO {
                    (old_total + new_total) / (pos.quantity + quantity)
                } else {
                    price
                };
                pos.quantity += quantity;
                pos.current_price = price;

                let trade = Trade {
                    id: self.next_trade_id,
                    symbol: symbol.clone(),
                    side,
                    price,
                    quantity,
                    cost,
                    fee,
                    timestamp: Utc::now(),
                    signal,
                    is_paper: true,
                };
                self.next_trade_id += 1;
                self.trade_history.push(trade.clone());
                Ok(trade)
            }
            Side::Sell => {
                let pos = self
                    .positions
                    .get_mut(symbol.as_str())
                    .ok_or_else(|| format!("no position in {}", symbol))?;

                if quantity > pos.quantity {
                    return Err(format!(
                        "insufficient quantity: have {} trying to sell {}",
                        pos.quantity, quantity
                    ));
                }

                let fee = cost * fee_rate;
                let proceeds = cost - fee;
                self.cash += proceeds;

                // Realize P&L
                let pnl = (price - pos.avg_entry_price) * quantity - fee;
                self.realized_pnl += pnl;

                pos.quantity -= quantity;
                pos.current_price = price;

                let trade = Trade {
                    id: self.next_trade_id,
                    symbol: symbol.clone(),
                    side,
                    price,
                    quantity,
                    cost,
                    fee,
                    timestamp: Utc::now(),
                    signal,
                    is_paper: true,
                };
                self.next_trade_id += 1;
                self.trade_history.push(trade.clone());

                // Remove position if fully closed
                if pos.quantity == Decimal::ZERO {
                    self.positions.remove(symbol.as_str());
                }

                Ok(trade)
            }
        }
    }

    /// Update prices for all positions (mark-to-market).
    pub fn update_prices(&mut self, prices: &HashMap<String, Decimal>) {
        for pos in self.positions.values_mut() {
            if let Some(price) = prices.get(pos.symbol.as_str()) {
                pos.current_price = *price;
                if pos.avg_entry_price > Decimal::ZERO {
                    pos.unrealized_pnl = (*price - pos.avg_entry_price) * pos.quantity;
                    pos.unrealized_pnl_pct =
                        ((*price - pos.avg_entry_price) / pos.avg_entry_price) * dec!(100);
                }
            }
        }
    }

    /// Take a snapshot of the portfolio.
    pub fn snapshot(&self, prices: &HashMap<String, Decimal>) -> PortfolioSnapshot {
        let mut total_value = self.cash;
        let mut unrealized = Decimal::ZERO;
        let positions: Vec<Position> = self
            .positions
            .values()
            .map(|p| {
                let current_price = prices
                    .get(p.symbol.as_str())
                    .copied()
                    .unwrap_or(p.current_price);
                let pnl = (current_price - p.avg_entry_price) * p.quantity;
                let value = current_price * p.quantity;
                total_value += value;
                unrealized += pnl;
                Position {
                    symbol: p.symbol.clone(),
                    quantity: p.quantity,
                    avg_entry_price: p.avg_entry_price,
                    current_price,
                    unrealized_pnl: pnl,
                    unrealized_pnl_pct: if p.avg_entry_price > Decimal::ZERO {
                        ((current_price - p.avg_entry_price) / p.avg_entry_price) * dec!(100)
                    } else {
                        Decimal::ZERO
                    },
                }
            })
            .collect();

        PortfolioSnapshot {
            total_value_usd: total_value,
            cash_usd: self.cash,
            positions,
            unrealized_pnl: unrealized,
            realized_pnl: self.realized_pnl,
            total_trades: self.trade_count(),
            win_rate: self.win_rate(),
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_signal(action: f64) -> HnnSignal {
        HnnSignal {
            attractor: "StableOrbit".into(),
            turbulence: 0.05,
            momentum: action,
            spike_rate: 0.0,
            action,
        }
    }

    #[test]
    fn new_portfolio() {
        let p = Portfolio::new(dec!(1000));
        assert_eq!(p.cash(), dec!(1000));
        assert_eq!(p.open_position_count(), 0);
    }

    #[test]
    fn buy_and_sell() {
        let mut p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");

        // Buy 0.01 BTC at $50,000
        let trade = p
            .execute_trade(
                btc.clone(),
                Side::Buy,
                dec!(50000),
                dec!(0.01),
                mock_signal(0.5),
            )
            .unwrap();
        assert_eq!(trade.side, Side::Buy);
        assert!(p.cash() < dec!(1000)); // cash reduced
        assert_eq!(p.open_position_count(), 1);

        // Sell 0.01 BTC at $55,000 (profit!)
        let trade = p
            .execute_trade(
                btc.clone(),
                Side::Sell,
                dec!(55000),
                dec!(0.01),
                mock_signal(-0.5),
            )
            .unwrap();
        assert_eq!(trade.side, Side::Sell);
        assert_eq!(p.open_position_count(), 0);

        // Realized P&L should be positive (roughly $50 minus fees)
        assert!(p.realized_pnl() > Decimal::ZERO);
    }

    #[test]
    fn insufficient_cash() {
        let mut p = Portfolio::new(dec!(100));
        let btc = Symbol::new("BTCUSDT");

        // Try to buy 1 BTC at $50,000 with only $100
        let result = p.execute_trade(btc, Side::Buy, dec!(50000), dec!(1), mock_signal(0.5));
        assert!(result.is_err());
    }

    #[test]
    fn insufficient_quantity() {
        let mut p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");

        p.execute_trade(
            btc.clone(),
            Side::Buy,
            dec!(50000),
            dec!(0.01),
            mock_signal(0.5),
        )
        .unwrap();

        // Try to sell more than we have
        let result = p.execute_trade(btc, Side::Sell, dec!(55000), dec!(0.02), mock_signal(-0.5));
        assert!(result.is_err());
    }

    #[test]
    fn snapshot_values() {
        let mut p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");
        p.execute_trade(
            btc.clone(),
            Side::Buy,
            dec!(50000),
            dec!(0.01),
            mock_signal(0.5),
        )
        .unwrap();

        let mut prices = HashMap::new();
        prices.insert("BTCUSDT".into(), dec!(55000));

        let snap = p.snapshot(&prices);
        assert!(snap.total_value_usd > dec!(1000)); // position appreciated
        assert!(snap.unrealized_pnl > Decimal::ZERO);
    }

    #[test]
    fn risk_limits_conservative() {
        let limits = RiskLimits::conservative();
        assert_eq!(limits.max_position_usd, dec!(50));
        assert_eq!(limits.max_open_positions, 3);
    }
}
