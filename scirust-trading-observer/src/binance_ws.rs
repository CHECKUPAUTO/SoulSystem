//! WebSocket client for Binance market data.

use scirust_trading_core::bus::EventBus;

/// Configuration for the Binance WebSocket observer.
#[derive(Clone)]
pub struct BinanceConfig {
    pub depth_speed_ms: u64,
    pub reconnect_delay_sec: u64,
    pub emit_interval_ms: u64,
    pub tick_bar_size: Option<u32>,
}

impl Default for BinanceConfig {
    fn default() -> Self {
        Self {
            depth_speed_ms: 100,
            reconnect_delay_sec: 2,
            emit_interval_ms: 500,
            tick_bar_size: Some(500),
        }
    }
}

/// Binance market data observer — placeholder.
pub struct BinanceObserver {
    _config: BinanceConfig,
    _bus: EventBus,
}

impl BinanceObserver {
    pub fn new(config: BinanceConfig, bus: EventBus) -> Self {
        Self {
            _config: config,
            _bus: bus,
        }
    }

    pub async fn run(&self) {
        // WebSocket connection loop — placeholder
    }
}
