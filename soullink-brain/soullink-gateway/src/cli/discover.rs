//! `gateway discover` — discover gateways via config probing.
//!
//! Reports the configured orchestrator URL and optionally probes it.
//! Future: Bonjour/mDNS discovery on LAN.

use std::process::ExitCode;

use serde_json::json;

use crate::cli::theme::{is_rich_tty, style_error, style_header, style_muted, style_success};
use crate::config;

pub struct DiscoverOpts {
    pub timeout: u64,
    pub json: bool,
}

pub async fn discover_cmd(opts: DiscoverOpts) -> ExitCode {
    let cfg = config::load_default().ok();

    if opts.json {
        let output = json!({
            "configured_url": cfg.as_ref().map(|c| c.orchestrator_url.as_str()).unwrap_or("none"),
            "telegram_configured": cfg.as_ref().and_then(|c| c.telegram.as_ref()).is_some(),
            "discovery_method": "config",
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return ExitCode::SUCCESS;
    }

    if is_rich_tty() {
        println!("{}", style_header("Gateway Discovery"));
    }
    println!("Discovery method: config file");

    match &cfg {
        Some(c) => {
            let has_telegram = c.telegram.is_some();
            if is_rich_tty() {
                println!(
                    "{}  Orchestrator: {}",
                    style_muted("│"),
                    style_success(&c.orchestrator_url)
                );
                println!(
                    "{}  Telegram:     {}",
                    style_muted("│"),
                    if has_telegram {
                        style_success("configured")
                    } else {
                        style_error("not configured")
                    }
                );
                println!("{}  Metrics port: {}", style_muted("│"), c.metrics_port);
            } else {
                println!("  Orchestrator: {}", c.orchestrator_url);
                println!(
                    "  Telegram:     {}",
                    if has_telegram {
                        "configured"
                    } else {
                        "not configured"
                    }
                );
                println!("  Metrics port: {}", c.metrics_port);
            }

            // Probe reachability
            let probe_url = format!("{}/health", c.orchestrator_url.trim_end_matches('/'));
            let client = soullink_core::http::shared_client();
            let timeout_dur = std::time::Duration::from_millis(opts.timeout);
            match client.get(&probe_url).timeout(timeout_dur).send().await {
                Ok(r) => {
                    if is_rich_tty() {
                        println!(
                            "{}  Reachable:    {}",
                            style_muted("│"),
                            style_success("yes")
                        );
                    } else {
                        println!("  Reachable:    yes (HTTP {})", r.status().as_u16());
                    }
                }
                Err(_) => {
                    if is_rich_tty() {
                        println!(
                            "{}  Reachable:    {}",
                            style_muted("│"),
                            style_error("no (timeout or unreachable)")
                        );
                    } else {
                        println!("  Reachable:    no");
                    }
                }
            }
        }
        None => {
            if is_rich_tty() {
                println!(
                    "{}  No config found at /etc/soullink/gateway.toml",
                    style_muted("│")
                );
            } else {
                println!("  No config found at /etc/soullink/gateway.toml");
            }
        }
    }

    ExitCode::SUCCESS
}
