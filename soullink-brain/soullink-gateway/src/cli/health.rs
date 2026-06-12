//! `gateway health` — ping orchestrator health endpoint.
//!
//! Reports reachability, HTTP status code, and response latency.

use std::process::ExitCode;

use serde_json::json;

use crate::cli::shared::format_duration;
use crate::cli::theme::{is_rich_tty, style_error, style_header, style_muted, style_success};
use crate::config;

pub struct HealthOpts {
    pub json: bool,
}

pub async fn health_cmd(opts: HealthOpts) -> ExitCode {
    let cfg = match config::load_default() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot load config: {e}");
            return ExitCode::from(2);
        }
    };

    let url = format!("{}/health", cfg.orchestrator_url.trim_end_matches('/'));
    let start = std::time::Instant::now();

    let client = soullink_core::http::shared_client();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    let elapsed = start.elapsed();

    match resp {
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();

            if opts.json {
                let output = json!({
                    "reachable": true,
                    "status_code": status.as_u16(),
                    "latency_ms": elapsed.as_millis(),
                    "body": body,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else if is_rich_tty() {
                let status_str = if status.is_success() {
                    style_success("UP")
                } else {
                    style_error(&format!("HTTP {}", status.as_u16()))
                };
                println!("{}", style_header("Orchestrator Health"));
                println!("{}  Reachable:   {}", style_muted("│"), status_str);
                println!(
                    "{}  Status:      HTTP {}",
                    style_muted("│"),
                    status.as_u16()
                );
                println!(
                    "{}  Latency:     {}",
                    style_muted("│"),
                    format_duration(elapsed)
                );
                println!(
                    "{}  Response:    {}",
                    style_muted("│"),
                    body.chars().take(120).collect::<String>()
                );
            } else {
                println!(
                    "Orchestrator: UP (HTTP {}, {}ms)",
                    status.as_u16(),
                    elapsed.as_millis()
                );
            }

            if status.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            if opts.json {
                let output = json!({
                    "reachable": false,
                    "latency_ms": elapsed.as_millis(),
                    "error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else if is_rich_tty() {
                println!("{}", style_header("Orchestrator Health"));
                println!("{}  Reachable:   {}", style_muted("│"), style_error("DOWN"));
                println!("{}  Error:       {}", style_muted("│"), e);
                println!(
                    "{}  Latency:     {}",
                    style_muted("│"),
                    format_duration(elapsed)
                );
            } else {
                println!("Orchestrator: DOWN ({})", e);
            }
            ExitCode::from(1)
        }
    }
}
