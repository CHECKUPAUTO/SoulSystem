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
//! soul-shell — Interactive CLI for SoulSystem kernel communication.
//!
//! Provides a REPL for direct interaction with the Brain, memory injection,
//! and manual stimulus injection.
//!
//! # Commands
//!
//! - `status` — Show organ status
//! - `inject <message>` — Inject a stimulus into the brain
//! - `memory <query>` — Query memory
//! - `health` — Check system health
//! - `help` — Show available commands
//! - `exit` — Exit the shell

use anyhow::{Context, Result};

// ── Command Model ───────────────────────────────────────────────────────

/// A command parsed from user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Show organ status.
    Status,
    /// Inject a stimulus.
    Inject(String),
    /// Query memory.
    Memory(String),
    /// Check system health.
    Health,
    /// Show turbulence history.
    Turbulence,
    /// Show bus events.
    Events,
    /// Show help.
    Help,
    /// Exit the shell.
    Exit,
    /// Unknown command.
    Unknown(String),
}

impl Command {
    /// Parse a command string.
    pub fn parse(input: &str) -> Self {
        let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.to_string());

        match cmd.as_str() {
            "status" | "s" => Command::Status,
            "inject" | "i" => Command::Inject(arg.unwrap_or_default()),
            "memory" | "mem" | "m" => Command::Memory(arg.unwrap_or_default()),
            "health" | "h" => Command::Health,
            "turbulence" | "t" => Command::Turbulence,
            "events" | "e" => Command::Events,
            "help" | "?" => Command::Help,
            "exit" | "quit" | "q" => Command::Exit,
            "" => Command::Unknown(String::new()),
            other => Command::Unknown(other.to_string()),
        }
    }

    /// Get help text for all commands.
    pub fn help_text() -> &'static str {
        "Available commands:
  status, s           Show organ status
  inject, i <msg>     Inject a stimulus into the brain
  memory, mem, m <q>  Query memory
  health, h           Check system health
  turbulence, t       Show turbulence history
  events, e           Show bus events
  help, ?             Show this help
  exit, quit, q       Exit the shell"
    }
}

// ── API Client ──────────────────────────────────────────────────────────

/// Client for communicating with the SoulSystem API.
pub struct SoulClient {
    base_url: String,
    http: reqwest::Client,
}

impl SoulClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Get system health.
    pub async fn health(&self) -> Result<serde_json::Value> {
        let url = format!("{}/health", self.base_url);
        let resp = self
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("Failed to connect to SoulSystem")?;

        if !resp.status().is_success() {
            anyhow::bail!("Health check returned status {}", resp.status());
        }

        let body = resp
            .json()
            .await
            .context("Failed to parse health response")?;
        Ok(body)
    }

    /// Send a stimulus to the brain.
    pub async fn inject(&self, message: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/stimulus", self.base_url);
        let body = serde_json::json!({
            "text": message,
            "source": "soul-shell",
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("Failed to inject stimulus")?;

        let body = resp
            .json()
            .await
            .context("Failed to parse injection response")?;
        Ok(body)
    }

    /// Query memory.
    pub async fn memory_query(&self, query: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/memory/query", self.base_url);
        let body = serde_json::json!({
            "query": query,
            "limit": 10,
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .context("Failed to query memory")?;

        let body = resp
            .json()
            .await
            .context("Failed to parse memory response")?;
        Ok(body)
    }
}

// ── Shell State ─────────────────────────────────────────────────────────

/// State maintained across shell sessions.
pub struct ShellState {
    pub command_history: Vec<String>,
    pub last_response: Option<serde_json::Value>,
    pub connected: bool,
}

impl ShellState {
    pub fn new() -> Self {
        Self {
            command_history: Vec::new(),
            last_response: None,
            connected: false,
        }
    }
}

impl Default for ShellState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Output Formatter ────────────────────────────────────────────────────

/// Format a response for display.
pub fn format_response(data: &serde_json::Value, indent: bool) -> String {
    if indent {
        serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
    } else {
        data.to_string()
    }
}

/// Format health status for display.
pub fn format_health(health: &serde_json::Value) -> String {
    let mut lines = Vec::new();

    if let Some(status) = health.get("status") {
        let color = match status.as_str() {
            Some("healthy") | Some("ok") => "♥",
            Some("degraded") => "⚠",
            _ => "✗",
        };
        lines.push(format!("{} Status: {}", color, status));
    }

    if let Some(organs) = health.get("organs").and_then(|o| o.as_array()) {
        lines.push(format!("Organs: {}", organs.len()));
        for organ in organs {
            let name = organ.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let healthy = organ
                .get("healthy")
                .and_then(|h| h.as_bool())
                .unwrap_or(false);
            let icon = if healthy { "♥" } else { "✗" };
            lines.push(format!("  {} {}", icon, name));
        }
    }

    lines.join("\n")
}

/// Format memory results for display.
pub fn format_memory_results(results: &serde_json::Value) -> String {
    let mut lines = Vec::new();

    if let Some(hits) = results.get("hits").and_then(|h| h.as_array()) {
        lines.push(format!("{} results:", hits.len()));
        for hit in hits {
            let content = hit
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("(no content)");
            let score = hit.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
            let source = hit.get("source").and_then(|s| s.as_str()).unwrap_or("?");
            lines.push(format!("  [{:.2}] ({}) {}", score, source, content));
        }
    } else {
        lines.push("No results found.".into());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parse() {
        assert_eq!(Command::parse("status"), Command::Status);
        assert_eq!(Command::parse("s"), Command::Status);
        assert_eq!(
            Command::parse("inject hello world"),
            Command::Inject("hello world".into())
        );
        assert_eq!(
            Command::parse("mem test query"),
            Command::Memory("test query".into())
        );
        assert_eq!(Command::parse("health"), Command::Health);
        assert_eq!(Command::parse("help"), Command::Help);
        assert_eq!(Command::parse("exit"), Command::Exit);
        assert_eq!(Command::parse("quit"), Command::Exit);
        assert_eq!(Command::parse(""), Command::Unknown(String::new()));
        assert_eq!(Command::parse("foo bar"), Command::Unknown("foo".into()));
    }

    #[test]
    fn test_help_text() {
        let help = Command::help_text();
        assert!(help.contains("status"));
        assert!(help.contains("inject"));
        assert!(help.contains("memory"));
        assert!(help.contains("exit"));
    }

    #[test]
    fn test_format_health() {
        let health = serde_json::json!({
            "status": "healthy",
            "organs": [
                {"name": "brain", "healthy": true},
                {"name": "memory", "healthy": false}
            ]
        });
        let formatted = format_health(&health);
        assert!(formatted.contains("healthy"));
        assert!(formatted.contains("brain"));
        assert!(formatted.contains("memory"));
    }

    #[test]
    fn test_format_memory_results() {
        let results = serde_json::json!({
            "hits": [
                {"content": "test content", "score": 0.95, "source": "memory"}
            ]
        });
        let formatted = format_memory_results(&results);
        assert!(formatted.contains("1 results"));
        assert!(formatted.contains("test content"));
        assert!(formatted.contains("0.95"));
    }

    #[test]
    fn test_shell_state() {
        let mut state = ShellState::new();
        assert!(!state.connected);
        assert!(state.command_history.is_empty());
        state.command_history.push("test".into());
        assert_eq!(state.command_history.len(), 1);
    }
}
