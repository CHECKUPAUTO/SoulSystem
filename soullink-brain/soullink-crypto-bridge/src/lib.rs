#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
//! SoulLink Crypto Bridge — TradingView MCP → HNN Crypto Organ
//!
//! Bridges TradingView Desktop (via tradingview-mcp CLI) to the
//! SoulLink crypto organ (port 9013). Provides:
//! - Market data ingestion via `tv stream` CLI
//! - Pine Script compilation and analysis
//! - Signal injection into the HNN mesh
//! - Decision feedback loop (mesh signals → chart actions)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

/// Configuration for the TradingView bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoBridgeConfig {
    /// Path to tradingview-mcp CLI (tv command)
    pub tv_cli_path: PathBuf,
    /// CDP port (default 9222)
    pub cdp_port: u16,
    /// Crypto organ URL
    pub organ_url: String,
    /// Symbols to monitor
    pub symbols: Vec<String>,
    /// Stream poll interval (ms)
    pub poll_interval_ms: u64,
}

impl Default for CryptoBridgeConfig {
    fn default() -> Self {
        Self {
            tv_cli_path: PathBuf::from("/mnt/nvme/soullink_brain/tradingview-mcp/src/cli/index.js"),
            cdp_port: 9222,
            organ_url: "http://127.0.0.1:9013".to_string(),
            symbols: vec!["BTCUSD".to_string(), "ETHUSD".to_string()],
            poll_interval_ms: 5000,
        }
    }
}

/// Market quote from TradingView.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQuote {
    pub symbol: String,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub change_pct: f64,
    pub timestamp: i64,
}

/// Signal to inject into the HNN crypto organ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoSignal {
    pub signal_type: String,
    pub symbol: String,
    pub strength: f64, // 0.0-1.0
    pub data: serde_json::Value,
}

/// The TradingView bridge.
pub struct CryptoBridge {
    config: CryptoBridgeConfig,
    client: reqwest::Client,
}

impl CryptoBridge {
    pub fn new(config: CryptoBridgeConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("reqwest client");
        Self { config, client }
    }

    /// Check if TradingView Desktop is connected via CDP.
    pub async fn health_check(&self) -> Result<bool, String> {
        let output = self.run_tv_command(&["status"]).await?;
        Ok(output.contains("\"connected\":true") || output.contains("connected"))
    }

    /// Get current quote for a symbol.
    pub async fn get_quote(&self, symbol: &str) -> Result<MarketQuote, String> {
        // Change to symbol first
        self.run_tv_command(&["symbol", symbol]).await?;

        // Get quote
        let output = self.run_tv_command(&["quote"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).map_err(|e| format!("parse quote: {e}"))?;

        Ok(MarketQuote {
            symbol: symbol.to_string(),
            close: parsed["close"].as_f64().unwrap_or(0.0),
            high: parsed["high"].as_f64().unwrap_or(0.0),
            low: parsed["low"].as_f64().unwrap_or(0.0),
            volume: parsed["volume"].as_f64().unwrap_or(0.0),
            change_pct: parsed["change_pct"].as_f64().unwrap_or(0.0),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Compile a Pine Script.
    pub async fn compile_pine(&self, source: &str) -> Result<Vec<String>, String> {
        // Set source then compile
        self.run_tv_command(&["pine", "set", source]).await?;
        let output = self.run_tv_command(&["pine", "compile"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).map_err(|e| format!("parse compile: {e}"))?;

        let errors = parsed["errors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(errors)
    }

    /// Capture chart screenshot.
    pub async fn screenshot(&self, region: &str) -> Result<Vec<u8>, String> {
        let output = self.run_tv_command(&["screenshot", "-r", region]).await?;
        // Base64 decode the screenshot data
        base64::decode(&output).map_err(|e| format!("decode screenshot: {e}"))
    }

    /// Stream quotes — returns a channel of market ticks.
    pub async fn stream_quotes(&self) -> Result<tokio::sync::mpsc::Receiver<MarketQuote>, String> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);

        let config = self.config.clone();
        tokio::spawn(async move {
            loop {
                match Self::run_tv_static(&config, &["stream", "quote"]).await {
                    Ok(output) => {
                        for line in output.lines() {
                            if let Ok(quote) = serde_json::from_str::<MarketQuote>(line) {
                                if tx.send(quote).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("stream error: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Inject a signal into the crypto organ.
    pub async fn inject_signal(&self, signal: &CryptoSignal) -> Result<(), String> {
        let url = format!("{}/api/stimulate", self.config.organ_url);
        self.client
            .post(&url)
            .json(signal)
            .send()
            .await
            .map_err(|e| format!("inject signal: {e}"))?;
        Ok(())
    }

    /// Get organ state.
    pub async fn get_organ_state(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/api/stats", self.config.organ_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("organ state: {e}"))?;
        resp.json().await.map_err(|e| format!("parse state: {e}"))
    }

    /// Convert market quote to HNN stimulus.
    pub fn quote_to_stimulus(quote: &MarketQuote, prev_close: f64) -> CryptoSignal {
        let price_change = if prev_close > 0.0 {
            (quote.close - prev_close) / prev_close
        } else {
            0.0
        };

        // Normalize to 0-1 strength
        let strength = (price_change.abs() * 100.0).min(1.0);

        CryptoSignal {
            signal_type: if price_change > 0.0 {
                "bullish"
            } else {
                "bearish"
            }
            .to_string(),
            symbol: quote.symbol.clone(),
            strength,
            data: serde_json::json!({
                "close": quote.close,
                "high": quote.high,
                "low": quote.low,
                "volume": quote.volume,
                "change_pct": quote.change_pct,
                "direction": if price_change > 0.0 { "up" } else { "down" },
            }),
        }
    }

    // ── Internal ─────────────────────────────────────────────────────

    async fn run_tv_command(&self, args: &[&str]) -> Result<String, String> {
        Self::run_tv_static(&self.config, args).await
    }

    async fn run_tv_static(config: &CryptoBridgeConfig, args: &[&str]) -> Result<String, String> {
        let output = Command::new("node")
            .arg(&config.tv_cli_path)
            .args(args)
            .env("TV_CDP_PORT", config.cdp_port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("spawn tv: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tv failed: {stderr}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let c = CryptoBridgeConfig::default();
        assert_eq!(c.cdp_port, 9222);
        assert_eq!(c.symbols.len(), 2);
        assert_eq!(c.poll_interval_ms, 5000);
    }

    #[test]
    fn quote_to_stimulus_bullish() {
        let quote = MarketQuote {
            symbol: "BTCUSD".into(),
            close: 95000.0,
            high: 96000.0,
            low: 94000.0,
            volume: 1000.0,
            change_pct: 2.5,
            timestamp: 0,
        };
        let sig = CryptoBridge::quote_to_stimulus(&quote, 90000.0);
        assert_eq!(sig.signal_type, "bullish");
        assert!(sig.strength > 0.0);
    }

    #[test]
    fn quote_to_stimulus_bearish() {
        let quote = MarketQuote {
            symbol: "ETHUSD".into(),
            close: 3000.0,
            high: 3200.0,
            low: 2900.0,
            volume: 500.0,
            change_pct: -3.0,
            timestamp: 0,
        };
        let sig = CryptoBridge::quote_to_stimulus(&quote, 3100.0);
        assert_eq!(sig.signal_type, "bearish");
        assert!(sig.strength > 0.0);
    }

    #[test]
    fn quote_to_stimulus_flat() {
        let quote = MarketQuote {
            symbol: "BTCUSD".into(),
            close: 90000.0,
            high: 90010.0,
            low: 89990.0,
            volume: 100.0,
            change_pct: 0.01,
            timestamp: 0,
        };
        let sig = CryptoBridge::quote_to_stimulus(&quote, 90000.0);
        assert_eq!(sig.signal_type, "bearish"); // 0 change = bearish by convention
        assert!(sig.strength < 0.01);
    }
}
