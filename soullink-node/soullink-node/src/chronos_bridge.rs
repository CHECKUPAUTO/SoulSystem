//! Chronos Bridge — Rust client for Chronos-RS MCP server.
//!
//! Chronos-RS runs a Model Context Protocol (MCP) server on port 9786
//! exposing screen capture, OCR timeline, and window context.
//! This bridge calls it directly via JSON-RPC over HTTP — no Python needed.

use reqwest::Client;
use serde_json::{json, Value};
use std::sync::LazyLock;

static MCP_URL: &str = "http://127.0.0.1:9786";

static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("chronos HTTP client")
});

/// Call a Chronos-RS MCP method.
async fn mcp_call(method: &str, params: Value) -> Result<Value, String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let resp = HTTP_CLIENT
        .post(&format!("{MCP_URL}/messages"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("chronos HTTP error: {e}"))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("chronos JSON parse: {e}"))?;

    if let Some(err) = body.get("error") {
        return Err(format!("chronos MCP error: {}", err));
    }

    Ok(body)
}

/// Extract text content from an MCP tool response.
fn extract_text(result: &Value) -> Option<String> {
    result
        .get("result")?
        .get("content")?
        .as_array()?
        .iter()
        .find_map(|item| {
            if item.get("type")?.as_str()? == "text" {
                item.get("text").and_then(|t| t.as_str().map(String::from))
            } else {
                None
            }
        })
}

/// Get current context (active window, recent OCR text).
pub async fn current_context() -> Result<Value, String> {
    let resp = mcp_call(
        "tools/call",
        json!({"name": "chronos_current_context", "arguments": {}}),
    )
    .await?;

    if let Some(text) = extract_text(&resp) {
        serde_json::from_str(&text).map_err(|e| format!("parse context: {e}"))
    } else {
        Ok(json!({"status": "no_context", "message": "No active context available"}))
    }
}

/// Query timeline events.
pub async fn query_timeline(
    from_ts: Option<&str>,
    to_ts: Option<&str>,
    limit: usize,
) -> Result<Value, String> {
    let resp = mcp_call(
        "tools/call",
        json!({
            "name": "chronos_query_timeline",
            "arguments": {
                "from": from_ts,
                "to": to_ts,
                "limit": limit,
            }
        }),
    )
    .await?;

    if let Some(text) = extract_text(&resp) {
        serde_json::from_str(&text).map_err(|e| format!("parse timeline: {e}"))
    } else {
        Ok(json!({"events": [], "total": 0}))
    }
}

/// Full-text search in OCR'd screen captures.
pub async fn search(query_str: &str) -> Result<Value, String> {
    let resp = mcp_call(
        "tools/call",
        json!({
            "name": "chronos_search",
            "arguments": { "query": query_str }
        }),
    )
    .await?;

    if let Some(text) = extract_text(&resp) {
        serde_json::from_str(&text).map_err(|e| format!("parse search: {e}"))
    } else {
        Ok(json!({"results": [], "query": query_str}))
    }
}

/// Get Chronos-RS statistics.
pub async fn stats() -> Result<Value, String> {
    let resp = mcp_call(
        "tools/call",
        json!({"name": "chronos_stats", "arguments": {}}),
    )
    .await?;

    if let Some(text) = extract_text(&resp) {
        serde_json::from_str(&text).map_err(|e| format!("parse stats: {e}"))
    } else {
        Ok(json!({"status": "unknown"}))
    }
}

/// Check if Chronos-RS is reachable.
pub async fn health_check() -> bool {
    HTTP_CLIENT
        .get(&format!("{MCP_URL}/health"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_handles_down() {
        // When chronos is down, health_check should return false
        let _ok = health_check().await;
        // Just verifying no panic occurs during the call
    }
}
