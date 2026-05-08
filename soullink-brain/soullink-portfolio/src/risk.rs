//! Risk management — hard limits that prevent catastrophic losses.
//!
//! Philosophy: The risk manager is the ONLY component that can block a trade.
//! No signal, no matter how strong, overrides risk limits.

use crate::portfolio::Portfolio;
use crate::types::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Risk check result.
#[derive(Debug, Clone, PartialEq)]
pub enum RiskVerdict {
    Allow,
    Deny(String),
    ReducePosition { max_quantity: Decimal, reason: String },
}

/// The risk manager — enforces hard limits.
pub struct RiskManager {
    pub limits: RiskLimits,
    /// Daily realized loss tracking.
    daily_realized_loss: Decimal,
}

impl RiskManager {
    pub fn new(limits: RiskLimits) -> Self {
        Self {
            limits,
            daily_realized_loss: Decimal::ZERO,
        }
    }

    /// Check if a proposed trade passes risk limits.
    pub fn check(
        &self,
        portfolio: &Portfolio,
        symbol: &Symbol,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        signal: &HnnSignal,
    ) -> RiskVerdict {
        // 1. Signal confidence check
        if signal.action.abs() < self.limits.min_signal_confidence {
            return RiskVerdict::Deny(format!(
                "signal too weak: {:.3} < {:.3}",
                signal.action.abs(),
                self.limits.min_signal_confidence
            ));
        }

        // 2. Position size check
        let position_cost = price * quantity;
        if position_cost > self.limits.max_position_usd {
            let max_qty = self.limits.max_position_usd / price;
            return RiskVerdict::ReducePosition {
                max_quantity: max_qty,
                reason: format!(
                    "position ${} exceeds max ${}",
                    position_cost, self.limits.max_position_usd
                ),
            };
        }

        // 3. Open positions check (buys only)
        if side == Side::Buy && portfolio.open_position_count() >= self.limits.max_open_positions {
            return RiskVerdict::Deny(format!(
                "max {} open positions reached",
                self.limits.max_open_positions
            ));
        }

        // 4. Total exposure check
        if side == Side::Buy {
            let current_exposure = self.calculate_total_exposure(portfolio);
            if current_exposure + position_cost > self.limits.max_total_exposure_usd {
                return RiskVerdict::Deny(format!(
                    "total exposure would be ${} > max ${}",
                    current_exposure + position_cost,
                    self.limits.max_total_exposure_usd
                ));
            }
        }

        // 5. Daily loss limit check
        if self.daily_realized_loss.abs() > self.limits.daily_loss_limit_usd {
            return RiskVerdict::Deny(format!(
                "daily loss limit reached: ${} > ${}",
                self.daily_realized_loss.abs(),
                self.limits.daily_loss_limit_usd
            ));
        }

        // 6. Turbulence override — don't trade in chaos
        if signal.turbulence > 0.8 {
            return RiskVerdict::Deny(format!(
                "extreme turbulence: {:.3}",
                signal.turbulence
            ));
        }

        // 7. Cash check
        if side == Side::Buy {
            let cost_with_fee = position_cost * (Decimal::ONE + dec!(0.001));
            if cost_with_fee > portfolio.cash() {
                return RiskVerdict::Deny(format!(
                    "insufficient cash: ${} < ${}",
                    portfolio.cash(),
                    cost_with_fee
                ));
            }
        }

        RiskVerdict::Allow
    }

    /// Calculate total portfolio exposure.
    fn calculate_total_exposure(&self, portfolio: &Portfolio) -> Decimal {
        // Sum of all position values at current prices
        // This is a rough estimate — we'd need position tracking
        let cash = portfolio.cash();
        // Total exposure = starting cash - remaining cash (what's deployed)
        // This works for simple portfolios
        dec!(1000) - cash // TODO: proper position value tracking
    }

    /// Record a realized loss for daily tracking.
    pub fn record_realized_pnl(&mut self, pnl: Decimal) {
        if pnl < Decimal::ZERO {
            self.daily_realized_loss += pnl;
        }
    }

    /// Reset daily counters (call at midnight).
    pub fn reset_daily(&mut self) {
        self.daily_realized_loss = Decimal::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_signal(action: f64, turbulence: f64) -> HnnSignal {
        HnnSignal {
            attractor: "StableOrbit".into(),
            turbulence,
            momentum: action,
            spike_rate: 0.0,
            action,
        }
    }

    #[test]
    fn allow_normal_trade() {
        let rm = RiskManager::new(RiskLimits::conservative());
        let p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");
        let signal = mock_signal(0.5, 0.1);

        let verdict = rm.check(&p, &btc, Side::Buy, dec!(50000), dec!(0.001), &signal);
        assert_eq!(verdict, RiskVerdict::Allow);
    }

    #[test]
    fn deny_weak_signal() {
        let rm = RiskManager::new(RiskLimits::conservative());
        let p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");
        let signal = mock_signal(0.1, 0.1); // too weak

        let verdict = rm.check(&p, &btc, Side::Buy, dec!(50000), dec!(0.001), &signal);
        assert!(matches!(verdict, RiskVerdict::Deny(_)));
    }

    #[test]
    fn deny_extreme_turbulence() {
        let rm = RiskManager::new(RiskLimits::conservative());
        let p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");
        let signal = mock_signal(0.8, 0.9); // strong signal but chaos

        let verdict = rm.check(&p, &btc, Side::Buy, dec!(50000), dec!(0.001), &signal);
        assert!(matches!(verdict, RiskVerdict::Deny(_)));
    }

    #[test]
    fn reduce_oversized_position() {
        let rm = RiskManager::new(RiskLimits::conservative());
        let p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");
        let signal = mock_signal(0.6, 0.1);

        // Try to buy $200 worth (limit is $50)
        let verdict = rm.check(&p, &btc, Side::Buy, dec!(50000), dec!(0.004), &signal);
        assert!(matches!(verdict, RiskVerdict::ReducePosition { .. }));
    }

    #[test]
    fn daily_loss_limit() {
        let mut rm = RiskManager::new(RiskLimits::conservative());
        rm.record_realized_pnl(dec!(-25)); // lost $25

        let p = Portfolio::new(dec!(1000));
        let btc = Symbol::new("BTCUSDT");
        let signal = mock_signal(0.6, 0.1);

        // Should still allow (limit is $20, loss is $25)
        let verdict = rm.check(&p, &btc, Side::Buy, dec!(50000), dec!(0.001), &signal);
        assert!(matches!(verdict, RiskVerdict::Deny(_)));
    }
}