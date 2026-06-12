//! # `souls` — Binaire principal de l'entité autonome
//!
//! Lance l'entité, expose le gateway HTTP/WS, démarre la boucle
//! autonome en arrière-plan, et optionnellement un REPL.
//!
//! ## Usage
//!
//! ```text
//! souls --gateway 127.0.0.1:7878 --memory /var/lib/souls/memory.db
//! souls --no-autonomous --no-gateway --repl
//! souls --config souls.toml
//! ```
//!
//! ## Variables d'environnement
//!
//! * `SOUL_GATEWAY_ADDR` — adresse du gateway (défaut: 127.0.0.1:7878)
//! * `SOUL_OLLAMA_URL`   — URL du serveur Ollama
//! * `SOUL_MEMORY_PATH`  — chemin de la base Sled
//! * `SOUL_AUTONOMOUS`   — "1" pour activer la boucle autonome
//! * `SOUL_RUST_LOG`     — niveau de log (`info`, `debug`, `warn`)

use clap::Parser;
use colored::Colorize;
use soul_entity::{EntityConfig, SoulEntity};
use soul_gateway::{serve as serve_gateway, GatewayState};
use soul_llm::{LlmConfig, ProviderKind};
use soul_openclaw::{Skill, SkillVersion};
use soul_sandbox::SandboxPolicy;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "souls", version, about = "SoulSystem — entité numérique autonome")]
struct Cli {
    /// Adresse du gateway HTTP/WS.
    #[arg(long, env = "SOUL_GATEWAY_ADDR", default_value = "127.0.0.1:7878")]
    gateway: String,

    /// Chemin de la mémoire persistante (Sled). Si omis, mémoire en RAM.
    #[arg(long, env = "SOUL_MEMORY_PATH")]
    memory: Option<PathBuf>,

    /// URL du serveur LLM (Ollama, OpenAI, etc.).
    #[arg(long, env = "SOUL_LLM_URL", default_value = "http://127.0.0.1:11434")]
    llm_url: String,

    /// Modèle LLM à utiliser.
    #[arg(long, default_value = "qwen3:8b")]
    model: String,

    /// Provider LLM (ollama, openai, anthropic).
    #[arg(long, env = "SOUL_LLM_PROVIDER", default_value = "ollama")]
    provider: ProviderKind,

    /// Clé API du provider (OpenAI, Anthropic, etc.).
    #[arg(long, env = "SOUL_LLM_API_KEY")]
    api_key: Option<String>,

    /// Active la boucle autonome en arrière-plan.
    #[arg(long, env = "SOUL_AUTONOMOUS", default_value_t = false)]
    autonomous: bool,

    /// Démarre le REPL interactif (terminal).
    #[arg(long, default_value_t = false)]
    repl: bool,

    /// Active la whitelist stricte du sandbox (binaire autorisé par défaut).
    #[arg(long, default_value_t = false)]
    strict_sandbox: bool,

    /// Tick (ms) de la boucle autonome.
    #[arg(long, default_value_t = 750)]
    tick_ms: u64,

    /// Nom de l'entité.
    #[arg(long, default_value = "soul")]
    name: String,

    /// Adresse du serveur métriques Prometheus.
    #[arg(long, env = "SOUL_METRICS_ADDR", default_value = "127.0.0.1:9090")]
    metrics: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init tracing
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,souls=debug".into());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(log_filter))
        .with_target(false)
        .init();

    let cli = Cli::parse();
    print_banner(&cli);

    // Init telemetry
    let telemetry_hub = Arc::new(soul_telemetry::TelemetryHub::new(num_cpus::get()));
    info!("telemetry hub initialisé pour {} cœurs", num_cpus::get());

    // 1. Construire l'entité
    let sandbox_policy = if cli.strict_sandbox {
        SandboxPolicy::strict(&["ls", "cat", "head", "tail", "grep", "find", "wc", "ps", "df", "free", "uname", "uptime", "whoami", "echo", "pwd", "date", "git", "cargo", "python3", "bash", "curl", "wc"])
    } else {
        SandboxPolicy::default()
    };

    let entity_config = EntityConfig {
        name: cli.name.clone(),
        llm: LlmConfig {
            provider: cli.provider,
            base_url: cli.llm_url.clone(),
            model: cli.model.clone(),
            temperature: 0.7,
            http_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            auth_token: cli.api_key.clone(),
            max_tokens: 2048,
            goal_token_budget: 50000,
            tokens_per_minute_budget: 100000,
            pool_max_idle: 10,
            pool_idle_timeout: Duration::from_secs(30),
        },
        sandbox_policy,
        loop_config: soul_openclaw::AgentLoopConfig::default(),
        autonomous_tick: Duration::from_millis(cli.tick_ms),
        memory_path: cli.memory.clone(),
        max_goal_history: 100,
        event_store_path: Some(std::path::PathBuf::from("/tmp/soul_events")),
    };

    let entity = Arc::new(SoulEntity::new(entity_config)?);
    info!("entité {} initialisée", cli.name);

    // 2. Enregistrer quelques skills openclaw
    entity.openclaw.skills.install(Skill::new(
        "system_info",
        SkillVersion::new(1, 0, 0),
        "Récupère les informations système de base",
    ));
    entity.openclaw.skills.install(Skill::new(
        "list_dir",
        SkillVersion::new(1, 0, 0),
        "Liste le contenu d'un répertoire",
    ));
    entity.openclaw.skills.install(Skill::new(
        "read_file",
        SkillVersion::new(1, 0, 0),
        "Lit un fichier texte",
    ));
    info!("skills initialisées: {}", entity.openclaw.skill_count());

    // 3. Créer un goal de démarrage pour amorcer la boucle autonome
    entity.create_goal("Vérifier l'état initial du système", 5);
    info!("goal de démarrage créé");

    // 4. Démarrer la boucle autonome en arrière-plan
    let entity_for_loop = entity.clone();
    let loop_handle = if cli.autonomous {
        let handle = tokio::spawn(async move {
            entity_for_loop.autonomous_loop().await;
        });
        info!("boucle autonome démarrée");
        Some(handle)
    } else {
        info!("boucle autonome désactivée (--no-autonomous)");
        None
    };

    // 5. Gateway HTTP/WS
    let gw_state = GatewayState::new(entity.clone() as Arc<dyn soul_gateway::EntityHandle>);
    let gw_addr: std::net::SocketAddr = cli
        .gateway
        .parse()
        .map_err(|e| anyhow::anyhow!("adresse gateway invalide: {e}"))?;

    let entity_for_status = entity.clone();
    let gateway_handle = tokio::spawn(async move {
        if let Err(e) = serve_gateway(gw_state, gw_addr).await {
            tracing::error!("gateway crashed: {e}");
        }
    });
    info!("gateway HTTP/WS sur http://{gw_addr}");

    // 6. Serveur métriques Prometheus
    let metrics_addr: SocketAddr = cli
        .metrics
        .parse()
        .map_err(|e| anyhow::anyhow!("adresse metrics invalide: {e}"))?;

    let hub_for_metrics = telemetry_hub.clone();
    let exporter = Arc::new(soul_telemetry::PrometheusExporter::new().expect("Failed to create exporter"));
    let metrics_handle = tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/metrics", axum::routing::get({
                let exporter = exporter.clone();
                move || {
                    let exporter = exporter.clone();
                    let hub = hub_for_metrics.clone();
                    async move {
                        exporter.update_from_hub(&hub);
                        match soul_telemetry::gather_metrics() {
                            Ok(metrics) => metrics,
                            Err(e) => {
                                tracing::error!("Failed to gather metrics: {e}");
                                String::from("# HELP metrics_error Failed to encode metrics\n")
                            }
                        }
                    }
                }
            }))
            .route("/health", axum::routing::get(|| async { "OK" }));

        let listener = tokio::net::TcpListener::bind(metrics_addr)
            .await
            .expect("Failed to bind metrics server");
        
        info!("serveur métriques Prometheus sur http://{metrics_addr}/metrics");
        
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("metrics server crashed: {e}");
        }
    });

    // 7. Serveur clinique (TCP HTTP léger) sur port+1 — partage l'auditor.
    let clinical_handle = match entity.subsystems.start_clinical_console({
        let mut p = gw_addr.port();
        p = p.saturating_add(1);
        p
    }) {
        Ok(h) => {
            info!("serveur clinique TCP sur port {}", gw_addr.port() + 1);
            Some(h)
        }
        Err(e) => {
            tracing::warn!("serveur clinique indisponible: {e}");
            None
        }
    };

    // 8. REPL optionnel (bloque si actif)
    if cli.repl {
        info!("démarrage du REPL interactif");
        let llm_cfg = LlmConfig {
            provider: cli.provider,
            base_url: cli.llm_url.clone(),
            model: cli.model.clone(),
            temperature: 0.7,
            http_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            auth_token: cli.api_key.clone(),
            max_tokens: 2048,
            goal_token_budget: 50000,
            tokens_per_minute_budget: 100000,
            pool_max_idle: 10,
            pool_idle_timeout: Duration::from_secs(30),
        };
        let mut repl_state = soul_repl::ReplState::new(llm_cfg);
        repl_state.entity_name = cli.name.clone();
        soul_repl::run_repl(&mut repl_state).await;
    }

    // 9. Attendre Ctrl+C
    match tokio::signal::ctrl_c().await {
        Ok(()) => info!("Ctrl+C reçu, arrêt en cours..."),
        Err(e) => tracing::error!("impossible d'installer ctrl_c: {e}"),
    }

    entity.stop();
    gateway_handle.abort();
    metrics_handle.abort();
    if let Some(h) = clinical_handle {
        h.shutdown();
    }
    if let Some(h) = loop_handle {
        h.abort();
    }

    println!("\n{}", "👋 SoulSystem arrêté proprement.".bright_cyan().bold());
    let final_status = entity_for_status.status();
    println!("{}", serde_json::to_string_pretty(&final_status).unwrap_or_default());
    Ok(())
}

fn print_banner(cli: &Cli) {
    let banner = format!(
        r"
╔══════════════════════════════════════════════════════════════╗
║   🧠  SoulSystem  —  Entité Numérique Autonome              ║
║   Framework : openclaw (openclaw.ai)                        ║
║   Nom       : {:<47} ║
║   Gateway   : {:<47} ║
║   Métriques : {:<47} ║
║   Ollama    : {:<47} ║
║   Modèle    : {:<47} ║
║   Mémoire   : {:<47} ║
║   Autonome  : {:<47} ║
╚══════════════════════════════════════════════════════════════╝
",
        cli.name,
        cli.gateway,
        cli.metrics,
        cli.llm_url,
        cli.model,
        cli.memory.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "/tmp".into()),
        if cli.autonomous { "oui" } else { "non" }
    );
    println!("{}", banner.bright_blue());
}