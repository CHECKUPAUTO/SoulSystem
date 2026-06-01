//! `gateway call` — RPC call to orchestrator or local gateway method.
//!
//! For local methods (health, status, providers.list, config.get, etc.),
//! calls the RPC registry directly. For remote methods, POST to the
//! orchestrator at `/api/mesh/{method}`.

use std::process::ExitCode;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::api::ApiState;
use crate::cli::theme::{is_rich_tty, style_error, style_header, style_muted, style_success};
use crate::config;
use crate::provider::registry::ProviderRegistry;
use crate::rpc::{RpcRegistry, RpcState};
use tracing::debug;

pub struct CallOpts {
    pub method: String,
    pub params: String,
    pub url: Option<String>,
    pub token: Option<String>,
    pub timeout: u64,
    pub json: bool,
}

pub async fn call_cmd(opts: CallOpts) -> ExitCode {
    let params: Value = match serde_json::from_str(&opts.params) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: invalid params JSON: {e}");
            return ExitCode::from(2);
        }
    };

    // For local calls (no --url), try the RPC registry first
    if opts.url.is_none() {
        let registry = Arc::new(ProviderRegistry::new());
        let rpc = RpcRegistry::build_default(registry.clone()).await;
        let state = RpcState {
            api: ApiState {
                registry: registry.clone(),
            },
            providers: registry,
        };

        match rpc.call(&opts.method, params.clone(), &state).await {
            Ok(payload) => {
                if opts.json {
                    println!("{}", serde_json::to_string_pretty(&payload).unwrap());
                } else if is_rich_tty() {
                    println!("{}", style_header(&format!("Call: {}", opts.method)));
                    println!("{}  Status: {}", style_muted("│"), style_success("OK"));
                    println!(
                        "{}  Response: {}",
                        style_muted("│"),
                        serde_json::to_string_pretty(&payload).unwrap_or_default()
                    );
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload).unwrap_or_default()
                    );
                }
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                // Fall through to HTTP
                debug!("local RPC failed: {e}, falling back to HTTP");
            }
        }
    }

    // Remote call: POST to orchestrator
    let orch_url = if let Some(ref u) = opts.url {
        u.clone()
    } else {
        config::load_default()
            .map(|c| c.orchestrator_url)
            .unwrap_or_else(|_| "http://127.0.0.1:9020".into())
    };

    let url = format!(
        "{}/api/mesh/{}",
        orch_url.trim_end_matches('/'),
        opts.method
    );
    let body = json!({ "method": opts.method, "params": params });

    let client = soullink_core::http::shared_client();
    let mut req = client
        .post(&url)
        .timeout(std::time::Duration::from_millis(opts.timeout))
        .json(&body);

    if let Some(t) = &opts.token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let response_body: Value = resp
                .json()
                .await
                .unwrap_or(json!({"raw": "non-JSON response"}));

            if opts.json {
                let output = json!({ "status_code": status.as_u16(), "response": response_body });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else if is_rich_tty() {
                let status_str = if status.is_success() {
                    style_success("OK")
                } else {
                    style_error(&format!("HTTP {}", status.as_u16()))
                };
                println!("{}", style_header(&format!("Call: {}", opts.method)));
                println!("{}  Status: {}", style_muted("│"), status_str);
                println!(
                    "{}  Response: {}",
                    style_muted("│"),
                    serde_json::to_string_pretty(&response_body).unwrap_or_default()
                );
            } else {
                println!("{} (HTTP {})", opts.method, status.as_u16());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&response_body).unwrap_or_default()
                );
            }
            if status.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("error: request failed: {e}");
            ExitCode::from(1)
        }
    }
}
