//! SoulLink Gateway — messaging bridge to the orchestrator.
//!
//! # Phase 6a scope
//!
//! * Telegram long-poll (no streaming, no media)
//! * Routes text messages through orchestrator `/api/mesh/think`
//! * Graceful shutdown on SIGTERM / Ctrl+C
//!
//! # What's NOT in Phase 6a
//!
//! * WhatsApp (Phase 6c)
//! * Streaming / SSE with message edits (Phase 6b)
//! * File/media forwarding (Phase 6b self-hosted / 6c WhatsApp)
//! * Webhook mode (long-poll only for now — webhooks need public HTTPS)

use std::process::ExitCode;

use tokio::sync::broadcast;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

use soullink_gateway::{
    config,
    orchestrator_bridge,
    telegram::{client::TelegramClient, long_poll::LongPollLoop},
};

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("soullink_gateway=info,warn"));
    fmt().with_env_filter(filter).with_target(true).init();
}

fn load_bot_token() -> Option<String> {
    // Priority: env var (for dev) → secrets.env (prod)
    if let Ok(t) = std::env::var("TELEGRAM_BOT_TOKEN") {
        if !t.is_empty() {
            info!("loaded Telegram bot token from TELEGRAM_BOT_TOKEN env var");
            return Some(t);
        }
    }

    // Read /etc/soullink/secrets.env for TELEGRAM_BOT_TOKEN=...
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
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some(value) = line.strip_prefix("TELEGRAM_BOT_TOKEN=") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                info!(path = %secrets_path, "loaded Telegram bot token from secrets.env");
                return Some(value.to_string());
            }
        }
    }
    warn!(path = %secrets_path, "TELEGRAM_BOT_TOKEN not present in secrets.env — Telegram disabled");
    None
}

#[tokio::main(worker_threads = 4)]
async fn main() -> ExitCode {
    init_tracing();

    let cfg = match config::load_default() {
        Ok(c) => c,
        Err(e) => {
            error!(%e, "gateway config load failed");
            return ExitCode::from(2);
        }
    };

    info!(
        orch = %cfg.orchestrator_url,
        has_telegram = cfg.telegram.is_some(),
        "SoulLink Gateway starting"
    );

    let (shutdown_tx, _) = broadcast::channel::<()>(4);

    // ── Telegram channel (optional) ─────────────────────────────────────
    let telegram_handle = if let Some(tg_cfg) = cfg.telegram.clone() {
        let token = match load_bot_token() {
            Some(t) => t,
            None => {
                error!("Telegram section present but token missing — refusing to start");
                return ExitCode::from(3);
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

        // Phase 6b: enable streaming if configured
        if tg_cfg.streaming {
            info!("Phase 6b streaming enabled — using /api/mesh/stream with progressive edits");
            let sc = std::sync::Arc::new(
                soullink_gateway::streaming_consumer::StreamingConsumer::new(
                    tg_client,
                    cfg.orchestrator_url.clone(),
                )
            );
            lp = lp.with_streaming(sc);
        } else {
            info!("Phase 6a blocking mode — /api/mesh/think with single sendMessage per reply");
        }

        Some(tokio::spawn(lp.run()))
    } else {
        info!("no [telegram] section in config — Telegram channel disabled");
        None
    };

    // ── Wait for shutdown ───────────────────────────────────────────────
    wait_for_shutdown().await;
    info!("shutdown signal received — stopping gateway");
    let _ = shutdown_tx.send(());

    if let Some(h) = telegram_handle {
        let _ = h.await;
    }

    info!("gateway exited cleanly");
    ExitCode::SUCCESS
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
