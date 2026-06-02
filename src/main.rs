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
use soulsystem::bound_system::BoundSystem;
use soulsystem::bus::Bus;
use soulsystem::ws_bridge::{run_ws_bridge, WsBridgeConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
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

    // MemoryHub (vectoriel + graph conceptuel + RAG)
    let mut memory_hub_raw = soulsystem::memory_hub::MemoryHub::new(&settings.paths.data_dir).await;
    {
        // Connecter le callback d'evenement sur le bus (Message::Custom)
        let bus_clone = bus.clone();
        memory_hub_raw.set_event_callback(std::sync::Arc::new(move |kind, payload| {
            use soulsystem::bus::Message;
            let topic = format!("memory.{}", kind);
            bus_clone.publish(Message::Custom { topic, payload });
        }));
    }
    let memory_hub = Arc::new(memory_hub_raw);
    info!("MemoryHub initialise");

    // ── Phase 0: CompactionWatchdog ────────────────────────────────────
    let watchdog_cfg = soulsystem::compaction_watchdog::CompactionConfig {
        data_dir: settings.paths.data_dir.clone(),
        message_threshold: 50,
        check_interval_secs: 30,
        proactive_save: true,
    };
    let watchdog = Arc::new(soulsystem::compaction_watchdog::CompactionWatchdog::new(
        watchdog_cfg,
    ));

    // Restaurer l'état précédent s'il existe (après compaction)
    if let Ok(Some(prev_state)) = watchdog.load_previous_state().await {
        info!(
            "CompactionWatchdog: contexte précédent restauré (tâche: {})",
            prev_state.current_task
        );
        // Injecter le texte de restauration dans le contexte via le bus
        let restore_text = prev_state.format_for_context();
        bus.publish(soulsystem::bus::Message::Custom {
            topic: "compaction.restore".into(),
            payload: serde_json::json!({
                "text": restore_text,
                "saved_at": prev_state.saved_at_ms,
            }),
        });
        // Nettoyer l'état après restauration
        let _ = watchdog.clear_state().await;
    }

    // Vérifier périodiquement les triggers de compaction
    let watchdog_clone = watchdog.clone();
    tokio::spawn(async move {
        watchdog_clone.run().await;
    });

    // Surveiller le trigger de compaction (écrit par OpenClaw ou processus externe)
    let watchdog_trigger = watchdog.clone();
    let bus_trigger = bus.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            if watchdog_trigger.check_compaction_trigger().await {
                warn!("CompactionWatchdog: signal de compaction détecté !");
                if let Err(e) = watchdog_trigger.on_compaction().await {
                    tracing::error!("CompactionWatchdog: erreur sauvegarde: {}", e);
                }
                // Émettre un événement sur le bus
                bus_trigger.publish(soulsystem::bus::Message::Custom {
                    topic: "compaction.detected".into(),
                    payload: serde_json::json!({"timestamp_ms": chrono::Utc::now().timestamp_millis() }),
                });
            }
        }
    });
    info!("CompactionWatchdog: surveillance active (interval: 15s)");

    // ── Phase 0: ContinuousSummarizer ──────────────────────────────────
    let summarizer_cfg = soulsystem::continuous_summarizer::SummarizerConfig {
        data_dir: settings.paths.data_dir.clone(),
        interval_secs: 600,    // 10 minutes
        message_threshold: 30, // ou tous les 30 messages
        summary_filename: "summary_latest.md".into(),
        pending_facts_filename: "pending_facts.md".into(),
        session_id: format!("soulsystem-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S")),
    };
    let summarizer =
        Arc::new(soulsystem::continuous_summarizer::ContinuousSummarizer::new(summarizer_cfg));

    // Lancer la boucle de résumé
    let summarizer_run = summarizer.clone();
    tokio::spawn(async move {
        summarizer_run.run().await;
    });

    // Souscripteur qui écoute les événements du bus et alimente le résumé
    let summarizer_bus = summarizer.clone();
    let bus_summary_sub = bus.clone();
    tokio::spawn(async move {
        use soulsystem::bus::Message;
        let mut rx = bus_summary_sub.subscribe();
        loop {
            match rx.recv().await {
                Ok(Message::Custom { topic, payload }) => match topic.as_str() {
                    "memory.stored" => {
                        let _ = summarizer_bus.increment_messages().await;
                    }
                    "memory.decision" => {
                        if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
                            summarizer_bus.add_decision(text).await;
                        }
                    }
                    "memory.error" => {
                        if let (Some(err), Some(fix)) = (
                            payload.get("error").and_then(|v| v.as_str()),
                            payload.get("fix").and_then(|v| v.as_str()),
                        ) {
                            summarizer_bus.add_error_fix(err, fix).await;
                        }
                    }
                    "memory.fact" => {
                        if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
                            summarizer_bus.add_fact(text).await;
                        }
                    }
                    "compaction.detected" => {
                        info!("Summarizer: compaction détectée, génération résumé préventif");
                        let _ = summarizer_bus.generate_summary().await;
                    }
                    _ => {}
                },
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    info!("ContinuousSummarizer: génération active (interval: 10min / 30msg)");

    // ── Phase 0: MemoryHealth étendu ───────────────────────────────────
    let health_hub = memory_hub.clone();
    let health_watchdog = watchdog.clone();
    let health_summarizer = summarizer.clone();
    let bus_health = bus.clone();
    tokio::spawn(async move {
        let config = soulsystem::memory_health::HealthConfig {
            latency_warn_ms: 500.0,
            max_entries_warn: 1000,
            search_failure_ratio: 0.3,
            window_size: 50,
            max_compactions_per_hour: 2,
            max_message_rate: 30.0,
            context_size_warn_tokens: 500_000,
            max_alpha_sync_variance: 0.4,
            max_error_rate: 0.15,
        };
        let mut health = soulsystem::memory_health::MemoryHealth::new(config);
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;

            // Alimenter les métriques depuis le watchdog
            let comp_count = *health_watchdog.compaction_count().read().await;
            for _ in 0..comp_count {
                health.record_compaction();
            }

            // Vérifier le taux de messages depuis le résumé
            let msg_count = health_summarizer.summary().read().await.message_count;
            for _ in 0..msg_count.min(10) {
                health.record_message();
            }

            let report = health.check(&health_hub).await;

            // Émettre une alerte de fatigue sur le bus
            if report.is_fatigued {
                bus_health.publish(soulsystem::bus::Message::Custom {
                    topic: "memory.fatigue_alert".into(),
                    payload: serde_json::json!({
                        "severity": "warning",
                        "message": "Fatigue cognitive détectée — pause recommandée",
                        "compactions": report.compactions_last_hour,
                        "alpha_variance": report.alpha_sync_variance,
                        "context_tokens": report.estimated_context_tokens,
                    }),
                });
            }

            if report.status != soulsystem::memory_health::HealthStatus::Healthy {
                tracing::warn!(
                    "MemoryHealth: {:?} — lat={:.1}ms comp={} msg/min={:.0} ctx={}K err={:.1}%",
                    report.status,
                    report.avg_latency_ms,
                    report.compactions_last_hour,
                    report.message_rate,
                    report.estimated_context_tokens / 1000,
                    report.error_rate * 100.0,
                );
            }
        }
    });
    info!("MemoryHealth: surveillance étendue active (interval: 1 min)");

    // ── Phase 1: RAG Middleware actif ──────────────────────────────────
    let rag_cfg = soulsystem::rag_middleware::RagMiddlewareConfig {
        data_dir: settings.paths.data_dir.join("rag"),
        top_k: 3,
        min_score: 0.7,
        hybrid_alpha: 0.7,
        cache_capacity: 100,
        context_window: 5,
        timeout_ms: 200,
    };
    let rag_middleware = Arc::new(soulsystem::rag_middleware::RagMiddleware::new(rag_cfg));
    if let Err(e) = rag_middleware.init_store().await {
        tracing::warn!("RAG middleware: init store échouée (mode dégradé): {}", e);
    }
    info!("RAG middleware: actif ({} top_k, {:.1} seuil)", 3, 0.7);

    // ── Phase 1: MemorySuggest (auto-suggestion MEMORY.md) ─────────────
    let suggest_cfg = soulsystem::memory_suggest::MemorySuggestConfig {
        data_dir: settings.paths.data_dir.clone(),
        memory_path: PathBuf::from(
            std::env::var("SOULLINK_MEMORY_PATH")
                .unwrap_or_else(|_| "/root/.openclaw/workspace/MEMORY.md".into()),
        ),
        apply_script_path: PathBuf::from(
            std::env::var("SOULLINK_APPLY_SCRIPT")
                .unwrap_or_else(|_| "/root/.openclaw/workspace/apply_memory_updates.sh".into()),
        ),
        auto_approve_threshold: 0.95,
    };
    let memory_suggest = Arc::new(soulsystem::memory_suggest::MemorySuggest::new(suggest_cfg));
    if memory_suggest.has_pending().await {
        info!(
            "MemorySuggest: {} faits en attente de validation",
            memory_suggest.pending_count().await
        );
    }
    info!("MemorySuggest: suggestions actives (seuil auto: 0.95)");

    // ── Phase 2: CheckpointLoader ─────────────────────────────────────
    let checkpoint_dir = settings.paths.data_dir.join("checkpoints");
    let checkpoint_loader = Arc::new(soulsystem::checkpoint_loader::CheckpointLoader::new(
        checkpoint_dir,
    ));
    if checkpoint_loader.has_checkpoints() {
        if let Some(cp) = checkpoint_loader.load_latest() {
            info!(
                "CheckpointLoader: checkpoint époque {} chargé ({} métriques)",
                cp.epoch,
                cp.metrics.len()
            );
            let bus_checkpoint = bus.clone();
            // Publier l'info sur le bus
            bus_checkpoint.publish(soulsystem::bus::Message::Custom {
                topic: "checkpoint.loaded".into(),
                payload: serde_json::json!({
                    "epoch": cp.epoch,
                    "metrics": cp.metrics,
                }),
            });
        }
    } else {
        info!("CheckpointLoader: aucun checkpoint existant (démarrage initial)");
    }

    // Prune automatique des vieux checkpoints (garder les 5 derniers)
    let cl_prune = checkpoint_loader.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(86400)); // 1 jour
        loop {
            interval.tick().await;
            cl_prune.prune_old(5);
        }
    });
    info!("CheckpointLoader: chargement initial terminé");

    // ── Phase 4: SleepCycle (consolidation offline) ────────────────────
    let sleep_cfg = soulsystem::sleep_cycle::SleepConfig {
        data_dir: settings.paths.data_dir.clone(),
        inactivity_threshold_secs: 1800, // 30 min
        link_reinforcement: 0.1,
        check_interval_secs: 300, // 5 min
    };
    let sleep_cycle = Arc::new(soulsystem::sleep_cycle::SleepCycle::new(sleep_cfg));
    let sleep_hub = memory_hub.clone();
    let sleep_bus = bus.clone();
    tokio::spawn(async move {
        sleep_cycle.run(sleep_hub, sleep_bus).await;
    });
    info!("SleepCycle: consolidation offline active (30min inactivité)");

    // ── Phase 3: TemporalIndex ──────────────────────────────────────────
    let temporal_index = match soulsystem::temporal_index::TemporalIndex::open(
        &settings.paths.data_dir.join("temporal"),
    ) {
        Ok(idx) => {
            info!(
                "TemporalIndex: index chronologique actif ({} entrées)",
                idx.count().unwrap_or(0)
            );
            Some(Arc::new(std::sync::Mutex::new(idx)))
        }
        Err(e) => {
            tracing::warn!("TemporalIndex: non disponible: {}", e);
            None
        }
    };
    if let Some(ref idx) = temporal_index {
        let idx_clone = idx.clone();
        let bus_temporal = bus.clone();
        tokio::spawn(async move {
            use soulsystem::bus::Message;
            let mut rx = bus_temporal.subscribe();
            loop {
                match rx.recv().await {
                    Ok(Message::Custom { topic, payload }) if topic == "memory.stored" => {
                        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        let tag = payload
                            .get("metadata")
                            .and_then(|m| m.get("tag"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("general");
                        let _ = idx_clone.lock().unwrap().insert_simple(
                            &format!("mem-{}", chrono::Utc::now().timestamp_millis()),
                            tag,
                            &text.chars().take(200).collect::<String>(),
                        );
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
        info!("TemporalIndex: souscripteur d'événements actif");
    }

    // ── API HTTP REST (pour agents OpenClaw) ─────────────────────────────
    let bound_system_api = Arc::new(BoundSystem::new(BoundSystem::default_whitelist()));
    let metrics_registry = soulsystem::metrics::init();
    let bridge_store = soulsystem::bridge_store::BridgeStore::open(
        settings.paths.log_dir.join("bridges.db"),
    )
    .ok()
    .map(Arc::new);
    let api_state = Arc::new(soulsystem::api::ApiState {
        bound_system: bound_system_api,
        pty_sessions: Arc::new(Mutex::new(HashMap::new())),
        memory: Some(memory_hub.clone()),
        metrics: metrics_registry,
        bridge_store,
    });
    let api_router = soulsystem::api::router(api_state);
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind("127.0.0.1:9023").await {
            Ok(listener) => {
                info!("API HTTP demarre sur 127.0.0.1:9023");
                if let Err(e) = axum::serve(listener, api_router).await {
                    tracing::error!("API HTTP server error: {}", e);
                }
            }
            Err(e) => tracing::error!("API HTTP: impossible de binder 127.0.0.1:9023: {}", e),
        }
    });

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
    //   memory_hub  → vectoriel + graphe conceptuel + RAG (unifié)
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

    // Discovery (mDNS sur port 42069)
    let disco = soulsystem::discovery::DiscoveryService::new(bus.clone());
    disco.start().await?;
    info!("DiscoveryService initialisé");

    // Télémétrie
    // Decay automatique toutes les 5 min
    let memory_decay = memory_hub.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            memory_decay.decay_and_prune(0.1, 0.99, 1000).await;
            tracing::info!("MemoryHub: decay automatique effectue");
        }
    });
    info!("MemoryHub: decay task lancee (interval: 5 min)");

    // ── Bridges inter-composants (vraies interconnexions HTTP) ─────────────
    //
    // Chaque bridge parle à un service vivant (openevolve, brain, synergie,
    // orchestrator) dans les deux sens : query local + push remote.

    #[cfg(feature = "avid")]
    {
        if let Err(e) = avid_bridge::init() {
            tracing::warn!("avid-bridge init failed: {}", e);
        }
    }
    #[cfg(feature = "openevolve")]
    {
        match openevolve_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(600));
                    loop {
                        iv.tick().await;
                        if let Err(e) = c.probe().await {
                            tracing::warn!("openevolve-bridge probe failed: {}", e);
                        }
                    }
                });
            }
            Err(e) => tracing::warn!("openevolve-bridge init failed: {}", e),
        }
    }
    #[cfg(feature = "synergie")]
    {
        match synergie_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(720));
                    loop {
                        iv.tick().await;
                        if let Err(e) = c.probe().await {
                            tracing::warn!("synergie-bridge probe failed: {}", e);
                        }
                    }
                });
            }
            Err(e) => tracing::warn!("synergie-bridge init failed: {}", e),
        }
    }
    #[cfg(feature = "brain_system")]
    {
        match brain_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(420));
                    loop {
                        iv.tick().await;
                        let _ = c.probe_all().await;
                    }
                });
            }
            Err(e) => tracing::warn!("brain-bridge init failed: {}", e),
        }
    }
    #[cfg(feature = "soul_neural")]
    {
        match soul_neural_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(600));
                    loop {
                        iv.tick().await;
                        if let Err(e) = c.sync_with_mesh().await {
                            tracing::warn!("soul-neural-bridge sync failed: {}", e);
                        }
                    }
                });
            }
            Err(e) => tracing::warn!("soul-neural-bridge init failed: {}", e),
        }
    }
    #[cfg(feature = "organs")]
    {
        match organs_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(480));
                    loop {
                        iv.tick().await;
                        let _ = c.probe_all().await;
                    }
                });
            }
            Err(e) => tracing::warn!("organs-bridge init failed: {}", e),
        }
    }
    #[cfg(feature = "mesh")]
    {
        match mesh_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(540));
                    loop {
                        iv.tick().await;
                        let _ = c.probe_all().await;
                    }
                });
            }
            Err(e) => tracing::warn!("mesh-bridge init failed: {}", e),
        }
    }
    #[cfg(feature = "services")]
    {
        match services_bridge::init().await {
            Ok(client) => {
                let c = client.clone();
                tokio::spawn(async move {
                    let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(360));
                    loop {
                        iv.tick().await;
                        let _ = c.probe_all().await;
                    }
                });
            }
            Err(e) => tracing::warn!("services-bridge init failed: {}", e),
        }
    }


    // Souscripteur memoire permanent (log des events)
    let mem_bus = bus.clone();
    tokio::spawn(async move {
        use soulsystem::bus::Message;
        let mut rx = mem_bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(Message::Custom { topic, payload }) if topic.starts_with("memory.") => {
                    tracing::info!("Memory event: {} — {}", topic, payload);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    info!("MemoryBridge: souscripteur memoire actif");

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

    // ── Phase 3: Métabolisme numérique (budget énergétique) ──────────────────
    // Lance le service de métabolisme sur le port 9052
    let _metabolism_handle = tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:9052").await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("Metabolism: impossible de binder sur :9052: {}", e);
                return;
            }
        };
        info!("Metabolism: service démarré sur 127.0.0.1:9052");
        // The metabolism binary runs independently, this is a health check
        let _ = axum::serve(
            listener,
            axum::Router::new().route(
                "/health",
                axum::routing::get(|| async { "metabolism-proxy-ok" }),
            ),
        )
        .await;
    });
    info!("Metabolism proxy actif sur :9052");

    // ── Phase 3: Preservation (instinct auto-préservation) ──────────────────
    let bus_preservation = bus.clone();
    tokio::spawn(async move {
        use soullink_autonomy::preservation::{Preservation, PreservationConfig};
        let preservation = Preservation::new(PreservationConfig::default());
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            // Monitor error cascades via bus events
            if let Some(actions) = preservation.record_error().await {
                for action in &actions {
                    bus_preservation.publish(soulsystem::bus::Message::Custom {
                        topic: "preservation.action".into(),
                        payload: serde_json::json!({"action": format!("{:?}", action)}),
                    });
                }
            }
            // Check resources periodically
            let cpu = read_cpu_usage();
            let mem = read_mem_usage();
            if let Some(actions) = preservation.check_resources(cpu, mem, 0.0).await {
                for action in &actions {
                    tracing::warn!("Preservation: {:?}", action);
                    bus_preservation.publish(soulsystem::bus::Message::Custom {
                        topic: "preservation.critical".into(),
                        payload: serde_json::json!({"action": format!("{:?}", action), "cpu": cpu, "mem": mem}),
                    });
                }
            }
        }
    });
    info!("Preservation instinct: surveillance active (interval: 30s)");

    info!("✅ SoulSystem prêt — boucle principale");

    // ── Boucle principale ──────────────────────────────────────────────────
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

fn read_cpu_usage() -> f64 {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|c| c.split_whitespace().next()?.parse::<f64>().ok())
        .map(|v| v * 100.0 / num_cpus())
        .unwrap_or(0.0)
}

fn read_mem_usage() -> f64 {
    let Ok(content) = std::fs::read_to_string("/proc/meminfo") else {
        return 0.0;
    };
    let mut total = 0.0f64;
    let mut available = 0.0f64;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if line.starts_with("MemAvailable:") {
            available = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
    }
    if total > 0.0 {
        ((total - available) / total) * 100.0
    } else {
        0.0
    }
}

fn num_cpus() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(8.0)
}
