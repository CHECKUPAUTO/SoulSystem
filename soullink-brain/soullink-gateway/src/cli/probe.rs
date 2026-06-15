//! `gateway probe` — combined health + status + connectivity check.
//!
//! Runs status display, orchestrator health probe, and config check
//! in one shot. Outputs a summary table with color.

use std::process::ExitCode;

use serde_json::json;

use crate::cli::theme::{
    is_rich_tty, style_error, style_header, style_muted, style_success, style_warn,
};
use crate::config;

pub struct ProbeOpts {
    pub url: Option<String>,
    pub json: bool,
}

pub async fn probe_cmd(opts: ProbeOpts) -> ExitCode {
    let cfg = config::load_default().ok();
    let orch_url = opts
        .url
        .or_else(|| cfg.as_ref().map(|c| c.orchestrator_url.clone()))
        .unwrap_or_else(|| "http://127.0.0.1:9020".into());

    let _results: Vec<serde_json::Value> = Vec::new();

    // ── Probe 1: orchestrator health ────────────────────────────────────
    let health_url = format!("{}/health", orch_url.trim_end_matches('/'));
    let start = std::time::Instant::now();
    let client = soullink_core::http::shared_client();
    let probe = client
        .get(&health_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;
    let health_elapsed = start.elapsed();

    let (healthy, health_status, _health_latency) = match probe {
        Ok(r) => (r.status().is_success(), r.status().as_u16(), health_elapsed),
        Err(_) => (false, 0, health_elapsed),
    };

    // ── Probe 2: config check ───────────────────────────────────────────
    let config_ok = cfg.is_some();
    let telegram_ok = cfg.as_ref().and_then(|c| c.telegram.as_ref()).is_some();
    let metrics_port = cfg.as_ref().map(|c| c.metrics_port).unwrap_or(0);

    if opts.json {
        let output = json!({
            "orchestrator": {
                "url": orch_url,
                "reachable": healthy,
                "status_code": health_status,
                "latency_ms": health_elapsed.as_millis(),
            },
            "config": {
                "loaded": config_ok,
                "telegram": telegram_ok,
                "metrics_port": metrics_port,
            },
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else if is_rich_tty() {
        println!("{}", style_header("Gateway Probe Summary"));
        println!("{}", style_muted("├─ Orchestrator"));
        let ok_str = if healthy {
            style_success("OK")
        } else {
            style_error("DOWN")
        };
        println!(
            "{}  {}  {:.1}ms  HTTP {}",
            style_muted("│"),
            ok_str,
            health_elapsed.as_millis() as f64,
            health_status
        );
        println!("{}", style_muted("├─ Config"));
        let cfg_str = if config_ok {
            style_success("loaded")
        } else {
            style_warn("missing")
        };
        println!(
            "{}  {}  Telegram={}",
            style_muted("│"),
            cfg_str,
            if telegram_ok { "on" } else { "off" }
        );
        println!("{}     Metrics port={}", style_muted("│"), metrics_port);
        println!("{}", style_muted("│"));
        println!("{}  Endpoint: {}", style_muted("└─"), orch_url);
    } else {
        println!("Probe Results:");
        println!(
            "  Orchestrator: {} (HTTP {}, {:.1}ms)",
            if healthy { "UP" } else { "DOWN" },
            health_status,
            health_elapsed.as_millis() as f64
        );
        println!("  Config: {}", if config_ok { "loaded" } else { "missing" });
        println!(
            "  Telegram: {}",
            if telegram_ok {
                "configured"
            } else {
                "not configured"
            }
        );
        println!("  Metrics port: {}", metrics_port);
    }

    if healthy && config_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
