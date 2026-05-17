//! SoulSystem — Point d'entrée principal (Operator Edition).
//!
//! Usage :
//!   soulsystem                          # démarrage normal
//!   soulsystem --dev                    # mode développement (dashboard + anomaly)
//!   soulsystem --mock                   # mode mock (simulation)
//!   soulsystem --version                # affiche la version
//!
//! Modules actifs : audit_log, bus, code_signing, compute_backend, config,
//!                  discovery, soul_memory, telemetry.

use anyhow::Result;
use clap::Parser;
use soulsystem::bus::Bus;
use soulsystem::ws_bridge::{run_ws_bridge, WsBridgeConfig};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "soulsystem",
    version = "0.5.0",
    about = "SoulSystem — Ecosysteme d'agent numerique autonome (Enriched Operator Edition)"
)]
struct Cli {
    /// Mode développement (dashboard web :9090 + détection anomalies)
    #[arg(long)]
    dev: bool,

    /// Mode mock (simulation uniquement)
    #[arg(long)]
    mock: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Chargement de la configuration centralisée
    let settings = soulsystem::config::Settings::new()?;
    info!(
        "SoulSystem v{} — config_dir: {:?}",
        env!("CARGO_PKG_VERSION"),
        settings.paths.config_dir
    );

    // Création des répertoires
    for dir in [
        &settings.paths.config_dir,
        &settings.paths.data_dir,
        &settings.paths.log_dir,
    ] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("Impossible de créer {:?}: {}", dir, e);
        }
    }

    // ── Bus central (file d'attente 256 messages) ──────────────────────────
    #[allow(unused_variables)]
    let bus = Arc::new(Bus::new(256));

    // ── WebSocket Bridge (plateforme unifiee Telegram) ───────────────────
    let bus_ws = bus.clone();
    tokio::spawn(async move {
        let config = WsBridgeConfig {
            listen: "127.0.0.1:9022".to_string(),
            shared_secret: None, // pas d'auth en local
        };
        run_ws_bridge(config, bus_ws).await;
    });
    info!("WS Bridge demarre sur 127.0.0.1:9022");

    // ── Bot Clawd Telegram ─────────────────────────────────────────────────
    let bot_token = std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default();
    if !bot_token.is_empty() {
        let bus_clawd = bus.clone();
        tokio::spawn(async move {
            let settings = soulsystem::clawd::Settings {
                bot_token,
                avid_endpoint: "http://localhost:7878".to_string(),
            };
            match soulsystem::clawd::ClawdContext::new(settings, bus_clawd) {
                Ok(ctx) => {
                    let ctx = Arc::new(ctx);
                    if let Err(e) = soulsystem::clawd::run_bot(ctx).await {
                        tracing::error!("Clawd bot error: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Clawd init error: {}", e);
                }
            }
        });
        info!("Clawd Telegram bot demarre");
    } else {
        info!("Clawd Telegram bot: SKIP (pas de TELEGRAM_BOT_TOKEN) — utilise OpenClaw Gateway");
    }

    // ── Modules actifs ─────────────────────────────────────────────────────
    //
    // Actifs en permanence (tous les modes) :
    //   audit_log    → journal d'audit signé
    //   code_signing → chaîne de certification
    //   bus          → messagerie interne
    //   compute_backend → CPU / GPU (CUDA / ROCm / Vulkan)
    //   config       → configuration centralisée
    //   discovery    → découverte de services réseau (mDNS)
    //   soul_memory  → mémoire vectorielle locale (sled + n-grammes)
    //   telemetry    → métriques OTLP
    //
    // Activés seulement en --dev :
    //   dev_dashboard → dashboard web sur :9090 (SSE)
    //   anomaly       → détecteur de chute de ticks HNN

    // Audit log (wrapped for shared access)
    let audit = Arc::new(Mutex::new(soulsystem::audit_log::AuditLog::open(
        &settings.paths.log_dir.join("audit.log").to_string_lossy(),
    )?));
    {
        let mut a = audit.lock().await;
        a.log("system", "startup", "SoulSystem Operator Edition demarre")?;
    }
    info!("AuditLogger initialise");

    // SoulMemory (vectorielle locale, fallback sled)
    let _memory = soulsystem::soul_memory::SoulMemory::new()?;
    info!("SoulMemory initialisée");

    // Discovery (mDNS sur port 42069)
    let mut disco = soulsystem::discovery::DiscoveryService::new(42069);
    disco.start().await?;
    info!("DiscoveryService initialisé");

    // Télémétrie
    let _ = soulsystem::telemetry::init_telemetry();
    info!("Telemetry initialisée");

    // ── Mode développement ─────────────────────────────────────────────────
    if cli.dev {
        info!("▶ Mode développement activé");

        #[cfg(feature = "dev")]
        {
            // Dashboard SSE sur :9090
            let bus_dash = bus.clone();
            let audit_dash = audit.clone();
            tokio::spawn(async move {
                if let Err(e) = soulsystem::dev_dashboard::run(bus_dash, audit_dash).await {
                    tracing::error!("Dashboard error: {}", e);
                }
            });

            // AnomalyWatcher — détection chute ticks HNN
            let bus_anom = bus.clone();
            tokio::spawn(async move {
                let mut watcher = soulsystem::anomaly::AnomalyWatcher::new(bus_anom);
                watcher.run().await;
            });
        }

        #[cfg(not(feature = "dev"))]
        {
            tracing::warn!("Feature 'dev' non activée — dashboard et anomaly désactivés");
            tracing::warn!("Recompilez avec --features dev");
        }
    }

    if cli.mock {
        info!("▶ Mode mock activé — simulation uniquement");
    }

    info!("✅ SoulSystem prêt — boucle principale");

    // ── Boucle principale ──────────────────────────────────────────────────
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        let count = disco.peers().len();
        debug_assert!(count <= 1024, "trop de pairs découverts");
        if count > 0 {
            tracing::debug!("{} pair(s) actif(s)", count);
        }
    }
}
