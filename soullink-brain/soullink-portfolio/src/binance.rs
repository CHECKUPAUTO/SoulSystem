//! Binance WebSocket client — real-time market data.
//!
//! Connects to Binance WebSocket API for live price feeds.
//! No API keys required for public market data streams.

use crate::types::*;
use chrono::Utc;
use futures::StreamExt;
use rust_decimal::Decimal;
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

/// Binance trade event from WebSocket.
#[derive(Debug, Deserialize)]
struct BinanceTrade {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "T")]
    #[allow(dead_code)]
    trade_time: i64,
}

/// Binance 24hr ticker for change data.
#[derive(Debug, Deserialize)]
struct BinanceTicker {
    #[serde(rename = "s")]
    #[allow(dead_code)]
    symbol: String,
    #[serde(rename = "P")]
    price_change_percent: String,
}

/// Connect to Binance WebSocket and stream ticks.
pub async fn stream_ticks(
    symbols: &[String],
    tx: tokio::sync::mpsc::Sender<Tick>,
) -> Result<(), String> {
    // Build stream URL: wss://stream.binance.com:9443/ws/btcusdt@trade/ethusdt@trade
    let streams: Vec<String> = symbols
        .iter()
        .map(|s| format!("{}@trade", s.to_lowercase()))
        .collect();

    let url = if streams.is_empty() {
        BINANCE_WS_URL.to_string()
    } else {
        format!("{}/{}", BINANCE_WS_URL, streams.join("/"))
    };

    info!("Connecting to Binance WS: {}", url);

    let (ws_stream, _) = connect_async(&url)
        .await
        .map_err(|e| format!("WS connect failed: {e}"))?;

    info!("Connected to Binance WebSocket");

    let (_, read) = ws_stream.split();

    read.for_each(|msg| {
        let tx = tx.clone();
        async move {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(trade) = serde_json::from_str::<BinanceTrade>(&text) {
                        if let (Ok(price), Ok(volume)) = (
                            Decimal::try_from(trade.price.as_str()),
                            Decimal::try_from(trade.volume.as_str()),
                        ) {
                            let tick = Tick {
                                symbol: Symbol::new(&trade.symbol),
                                price,
                                volume,
                                timestamp: Utc::now(),
                                change_24h_pct: Decimal::ZERO, // filled from REST ticker
                            };
                            if tx.send(tick).await.is_err() {
                                warn!("Tick channel closed");
                            }
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("Ping received: {} bytes", data.len());
                }
                Err(e) => {
                    error!("WS error: {e}");
                }
                _ => {}
            }
        }
    })
    .await;

    Err("WebSocket stream ended".into())
}

/// Fetch 24hr ticker data from Binance REST API.
/// Used to get price change % which isn't in the trade stream.
pub async fn fetch_24h_tickers(
    client: &reqwest::Client,
    symbols: &[String],
) -> Result<std::collections::HashMap<String, Decimal>, String> {
    let mut changes = std::collections::HashMap::new();

    for symbol in symbols {
        let url = format!(
            "https://api.binance.com/api/v3/ticker/24hr?symbol={}",
            symbol
        );
        match client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(ticker) = resp.json::<BinanceTicker>().await {
                    if let Ok(pct) = ticker.price_change_percent.parse::<f64>() {
                        changes.insert(
                            symbol.clone(),
                            Decimal::try_from(pct).unwrap_or(Decimal::ZERO),
                        );
                    }
                }
            }
            Err(e) => warn!("Failed to fetch ticker for {}: {e}", symbol),
        }
    }

    Ok(changes)
}
