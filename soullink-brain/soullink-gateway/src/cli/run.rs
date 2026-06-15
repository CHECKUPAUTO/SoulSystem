//! `gateway run` — foreground gateway process.
//!
//! Startup sequence:
//!   1. Parse CLI args → merge with TOML config
//!   2. Acquire PID lock (prevents duplicate instances)
//!   3. Init metrics + Prometheus exporter
//!   4. Emit banner
//!   5. Load & register providers
//!   6. Start Telegram client (if configured)
//!   7. Start axum HTTP server (health + API + metrics)
//!   8. Wait for shutdown → graceful cleanup

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

use crate::api::{self, ApiState};
use crate::cli::banner::emit_banner;
use crate::config;
use crate::lock::GatewayLock;
use crate::orchestrator_bridge;
use crate::provider;
use crate::telegram::{client::TelegramClient, long_poll::LongPollLoop};

/// CLI fields from the `Run` subcommand.
pub struct RunOpts {
    pub port: u16,
    pub bind: String,
    pub token: Option<String>,
    pub auth: Option<String>,
    pub password: Option<String>,
    pub password_file: Option<String>,
    pub config_path: Option<String>,
    pub orchestrator_url: Option<String>,
    pub verbose: bool,
    pub json: bool,
}

fn init_tracing(verbose: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("soullink_gateway=debug,info"))
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("soullink_gateway=info,warn"))
    };
    fmt().with_env_filter(filter).with_target(true).init();
}

fn load_bot_token(cli_token: Option<String>) -> Option<String> {
    if let Some(t) = cli_token {
        if !t.is_empty() {
            info!("using Telegram bot token from CLI --token");
            return Some(t);
        }
    }

    if let Ok(t) = std::env::var("TELEGRAM_BOT_TOKEN") {
        if !t.is_empty() {
            info!("loaded Telegram bot token from TELEGRAM_BOT_TOKEN env var");
            return Some(t);
        }
    }

    let secrets_path = std::env::var("SOULLINK_SECRETS_PATH")
        .unwrap_or_else(|_| "/etc/soullink/secrets.env".into());
    let raw = match std::fs::read_to_string(&secrets_path) {
        Ok(r) => r,
        Err(e) => {
            warn!(path = %secrets_path, %e, "secrets file unreadable — Telegram disabled");
            return None;
        }
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("TELEGRAM_BOT_TOKEN=") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                info!(path = %secrets_path, "loaded Telegram bot token from secrets.env");
                return Some(value.to_string());
            }
        }
    }
    warn!(path = %secrets_path, "TELEGRAM_BOT_TOKEN not present in secrets.env");
    None
}

fn resolve_config(opts: &RunOpts) -> config::GatewayConfig {
    let mut cfg = config::load_default().unwrap_or_default();
    if let Some(url) = &opts.orchestrator_url {
        cfg.orchestrator_url = url.clone();
    }
    if opts.port != 9092 {
        cfg.metrics_port = opts.port;
    }
    cfg
}

pub async fn run_cmd(opts: RunOpts) -> std::process::ExitCode {
    init_tracing(opts.verbose);
    emit_banner();

    let cfg = resolve_config(&opts);

    // ── Phase 1: PID lock ──────────────────────────────────────────────
    metrics::counter!(crate::metrics::LOCK_ATTEMPTS).increment(1);
    let pid_dir = std::env::var("SOULLINK_PID_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    let gate_port = opts.port;

    let mut lock = match GatewayLock::acquire(gate_port, pid_dir) {
        Ok(l) => {
            info!(port = gate_port, "gateway lock acquired");
            l
        }
        Err(e) => {
            metrics::counter!(crate::metrics::LOCK_FAILURES).increment(1);
            error!(err = %e, "gateway lock acquisition failed");
            eprintln!("error: {e}");
            return std::process::ExitCode::from(1);
        }
    };

    // ── Phase 2: Metrics ──────────────────────────────────────────────
    crate::metrics::register_descriptions();
    let metrics_addr = format!("127.0.0.1:{}", cfg.metrics_port);
    let metrics_handle: Option<()> = match metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(
            metrics_addr
                .parse::<std::net::SocketAddr>()
                .expect("metrics addr"),
        )
        .install()
    {
        Ok(_) => {
            info!(addr = %metrics_addr, "Prometheus metrics endpoint");
            None // exporter runs in background, no handle needed
        }
        Err(e) => {
            warn!(err = %e, "Prometheus exporter install failed — metrics disabled, continuing");
            None
        }
    };
    let _ = metrics_handle; // exporter runs in background

    info!(
        orch = %cfg.orchestrator_url,
        has_telegram = cfg.telegram.is_some(),
        "SoulLink Gateway RS starting"
    );

    let (shutdown_tx, _) = broadcast::channel::<()>(4);

    // ── Phase 3: Provider registry ────────────────────────────────────
    let provider_configs = provider::config::load_provider_configs();
    let provider_registry = Arc::new(provider::registry::ProviderRegistry::new());

    for (name, pcfg) in &provider_configs {
        provider_registry.register(name, pcfg.clone()).await;
    }

    if provider_registry.len().await == 0 {
        info!("no LLM providers configured — /api/chat will return 503");
    } else {
        info!(
            count = provider_registry.len().await,
            "providers registered"
        );
    }

    // ── Phase 4: Telegram channel (optional) ───────────────────────────
    let telegram_handle = if let Some(tg_cfg) = cfg.telegram.clone() {
        let token = match load_bot_token(opts.token.clone()) {
            Some(t) => t,
            None => {
                error!("Telegram section present but token missing");
                lock.release();
                return std::process::ExitCode::from(3);
            }
        };

        let tg_client = TelegramClient::new(tg_cfg.api_base_url.clone(), token);
        let bridge = orchestrator_bridge::build_bridge(
            cfg.orchestrator_url.clone(),
            cfg.orchestrator_timeout,
        );
        let mut lp = LongPollLoop::new(
            tg_client.clone(),
            bridge,
            tg_cfg.long_poll_timeout_s,
            tg_cfg.allowed_chat_ids,
            tg_cfg.typing_pre_reply,
            shutdown_tx.subscribe(),
        );

        if tg_cfg.streaming {
            info!("Phase 6b streaming enabled — /api/mesh/stream with progressive edits");
            let sc = Arc::new(crate::streaming_consumer::StreamingConsumer::new(
                tg_client,
                cfg.orchestrator_url.clone(),
            ));
            lp = lp.with_streaming(sc);
        } else {
            info!("Phase 6a blocking mode — /api/mesh/think with single sendMessage per reply");
        }

        Some(tokio::spawn(lp.run()))
    } else {
        info!("no [telegram] section in config — Telegram channel disabled");
        None
    };

    // ── Phase 5: HTTP server (health + API + metrics) ──────────────────
    // OpenClaw-compatible env: `PORT` overrides --port, `GATEWAY_TOKEN`
    // supplies the operator token when --token is not given.
    let effective_port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(opts.port);
    let gateway_token = opts
        .token
        .clone()
        .or_else(|| std::env::var("GATEWAY_TOKEN").ok());
    if gateway_token.is_none() {
        warn!("GATEWAY_TOKEN not set — gateway running without static operator token");
    }

    // Resolve the bind host: `--bind` (or the `GATEWAY_BIND` env, which wins)
    // maps loopback/local → 127.0.0.1 and all/any/public/0.0.0.0 → 0.0.0.0
    // (the latter matches the OpenClaw reference, which binds 0.0.0.0). Any
    // other value is used verbatim as the host.
    let bind_spec = std::env::var("GATEWAY_BIND").unwrap_or_else(|_| opts.bind.clone());
    let bind_host = match bind_spec.trim().to_lowercase().as_str() {
        "loopback" | "local" | "localhost" | "127.0.0.1" => "127.0.0.1".to_string(),
        "all" | "any" | "public" | "0.0.0.0" | "*" => "0.0.0.0".to_string(),
        other => other.to_string(),
    };

    let http_handle = if effective_port > 0 {
        let bind_addr = format!("{bind_host}:{effective_port}");

        // Build API router with provider registry + WebSocket
        let ws_store = std::sync::Arc::new(crate::ws::session::SessionStore::new());
        let auth = std::sync::Arc::new(crate::ws::auth::AuthManager::new(gateway_token));
        let api_state = ApiState::new(provider_registry.clone());
        let api_routes = api::build_router(api_state);
        let ws_store_clone = ws_store.clone();
        let auth_clone = auth.clone();
        let api_state_clone = ApiState::new(provider_registry.clone());

        // OpenClaw-compatible `/status` endpoint.
        let status_store = ws_store.clone();
        let status_port = effective_port;
        let app = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "OK" }))
            .route(
                "/status",
                axum::routing::get(move || {
                    let store = status_store.clone();
                    async move {
                        axum::response::Json(serde_json::json!({
                            "version": env!("CARGO_PKG_VERSION"),
                            "sessions": store.len().await,
                            "port": status_port,
                        }))
                    }
                }),
            )
            .route(
                "/ws",
                axum::routing::get(move |ws: axum::extract::ws::WebSocketUpgrade| {
                    let store = ws_store_clone.clone();
                    let auth = auth_clone.clone();
                    let api = std::sync::Arc::new(api_state_clone.clone());
                    async move {
                        ws.on_upgrade(move |socket| {
                            crate::ws::handler::handle_connection(
                                socket,
                                store,
                                api,
                                auth,
                                tokio::sync::broadcast::channel(1).1,
                            )
                        })
                    }
                }),
            )
            .merge(api_routes)
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tower_http::trace::TraceLayer::new_for_http());

        let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
            Ok(l) => Some(l),
            Err(e) => {
                warn!(addr = %bind_addr, %e, "HTTP server bind failed");
                None
            }
        };

        if let Some(listener) = listener {
            info!(addr = %bind_addr, "HTTP server + API listening");
            Some(tokio::spawn(async move {
                axum::serve(listener, app).await.ok();
            }))
        } else {
            None
        }
    } else {
        None
    };

    // ── Phase 6: Wait for shutdown ────────────────────────────────────
    wait_for_shutdown().await;
    info!("shutdown signal received — stopping gateway");
    let _ = shutdown_tx.send(());

    if let Some(h) = telegram_handle {
        let _ = h.await;
    }
    drop(http_handle);

    // Release PID lock
    lock.release();
    info!("gateway exited cleanly");
    std::process::ExitCode::SUCCESS
}

async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => info!("received SIGTERM"),
            _ = sigint.recv()  => info!("received SIGINT"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
