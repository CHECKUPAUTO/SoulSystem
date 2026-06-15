//! `gateway status` — show gateway service status.
//!
//! Reads the TOML config and displays orchestrator URL, Telegram status,
//! and metrics port. With `--probe`, pings the orchestrator health endpoint.

use std::process::ExitCode;

use serde_json::json;
use tracing::info;

use crate::cli::theme::{is_rich_tty, style_error, style_header, style_muted, style_success};
use crate::config;

pub struct StatusOpts {
    pub probe: bool,
    pub json: bool,
}

/// Health probe result from the orchestrator.
struct ProbeResult {
    reachable: bool,
    latency_ms: u64,
    status_code: u16,
}

async fn probe_orchestrator(url: &str) -> ProbeResult {
    let start = std::time::Instant::now();
    let client = soullink_core::http::shared_client();
    let resp = client
        .get(format!("{}/health", url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match resp {
        Ok(r) => {
            let latency = start.elapsed();
            ProbeResult {
                reachable: r.status().is_success(),
                latency_ms: latency.as_millis() as u64,
                status_code: r.status().as_u16(),
            }
        }
        Err(_) => ProbeResult {
            reachable: false,
            latency_ms: start.elapsed().as_millis() as u64,
            status_code: 0,
        },
    }
}

pub async fn status_cmd(opts: StatusOpts) -> ExitCode {
    let cfg = match config::load_default() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot load config: {e}");
            return ExitCode::from(2);
        }
    };

    let telegram_status = match &cfg.telegram {
        Some(tg) => {
            let allowed = tg
                .allowed_chat_ids
                .as_ref()
                .map(|ids| format!("{} chat(s)", ids.len()))
                .unwrap_or_else(|| "all".into());
            format!("enabled (allowed: {allowed})")
        }
        None => "disabled".into(),
    };

    if opts.json {
        let output = json!({
            "orchestrator_url": cfg.orchestrator_url,
            "telegram": telegram_status,
            "metrics_port": cfg.metrics_port,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return ExitCode::SUCCESS;
    }

    if is_rich_tty() {
        println!("{}", style_header("Gateway Status"));
        println!(
            "{}  Orchestrator URL:  {}",
            style_muted("│"),
            cfg.orchestrator_url
        );
        println!(
            "{}  Telegram:          {}",
            style_muted("│"),
            telegram_status
        );
        println!(
            "{}  Metrics port:      {}",
            style_muted("│"),
            cfg.metrics_port
        );
        info!("status check complete");
    } else {
        println!("Gateway Status");
        println!("  Orchestrator URL:  {}", cfg.orchestrator_url);
        println!("  Telegram:          {}", telegram_status);
        println!("  Metrics port:      {}", cfg.metrics_port);
    }

    // Optional probe
    if opts.probe {
        if is_rich_tty() {
            println!("{}", style_muted("│"));
            println!(
                "{}  {}",
                style_muted("├─"),
                style_header("Orchestrator Probe")
            );
        } else {
            println!("  Orchestrator Probe:");
        }

        let result = probe_orchestrator(&cfg.orchestrator_url).await;
        if is_rich_tty() {
            let status_str = if result.reachable {
                style_success("UP")
            } else {
                style_error("DOWN")
            };
            println!(
                "{}  {}  reachable: {} (HTTP {}, {}ms)",
                style_muted("│"),
                style_muted("├─"),
                status_str,
                result.status_code,
                result.latency_ms
            );
        } else {
            println!(
                "    reachable: {} (HTTP {}, {}ms)",
                if result.reachable { "UP" } else { "DOWN" },
                result.status_code,
                result.latency_ms
            );
        }
    }

    ExitCode::SUCCESS
}
