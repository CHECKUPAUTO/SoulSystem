//! # `souls` — Binaire principal de l'entité autonome
//!
//! Lance l'entité, expose le gateway HTTP/WS, démarre la boucle
//! autonome en arrière-plan, et optionnellement un REPL.
//!
//! ## Usage
//!
//! ```text
//! souls                                     # lance avec config par défaut
//! souls run --repl                          # lance avec REPL interactif
//! souls config                             # menu interactif de configuration
//! souls config --show                      # affiche la config actuelle
//! souls run --provider openai --model gpt-4o
//! ```

pub mod brain;
pub mod config;

use clap::{Parser, Subcommand};
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
#[command(
    name = "souls",
    version,
    about = "SoulSystem — entité numérique autonome",
    long_about = "SoulSystem est un framework d'entités numériques autonomes avec support multi-fournisseurs LLM."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Adresse du gateway HTTP/WS.
    #[arg(
        long,
        env = "SOUL_GATEWAY_ADDR",
        default_value = "127.0.0.1:7878",
        global = true
    )]
    gateway: String,

    /// Chemin de la mémoire persistante (Sled). Si omis, mémoire en RAM.
    #[arg(long, env = "SOUL_MEMORY_PATH", global = true)]
    memory: Option<PathBuf>,

    /// URL du serveur LLM (Ollama, OpenAI, etc.).
    #[arg(
        long,
        env = "SOUL_LLM_URL",
        default_value = "http://127.0.0.1:11434",
        global = true
    )]
    llm_url: String,

    /// Modèle LLM à utiliser.
    #[arg(long, default_value = "qwen3:8b", global = true)]
    model: String,

    /// Provider LLM (ollama, openai, anthropic).
    #[arg(
        long,
        env = "SOUL_LLM_PROVIDER",
        default_value = "ollama",
        global = true
    )]
    provider: ProviderKind,

    /// Clé API du provider (OpenAI, Anthropic, etc.).
    #[arg(long, env = "SOUL_LLM_API_KEY", global = true)]
    api_key: Option<String>,

    /// Active la boucle autonome en arrière-plan.
    #[arg(long, env = "SOUL_AUTONOMOUS", default_value_t = false, global = true)]
    autonomous: bool,

    /// Démarre le REPL interactif (terminal).
    #[arg(long, default_value_t = false, global = true)]
    repl: bool,

    /// Active la whitelist stricte du sandbox.
    #[arg(long, default_value_t = false, global = true)]
    strict_sandbox: bool,

    /// Tick (ms) de la boucle autonome.
    #[arg(long, default_value_t = 750, global = true)]
    tick_ms: u64,

    /// Nom de l'entité.
    #[arg(long, default_value = "soul", global = true)]
    name: String,

    /// Adresse du serveur métriques Prometheus.
    #[arg(
        long,
        env = "SOUL_METRICS_ADDR",
        default_value = "127.0.0.1:9090",
        global = true
    )]
    metrics: String,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lance l'entité avec le gateway et la boucle autonome.
    Run,
    /// Ouvre le TUI interactif (terminal riche).
    Tui,
    /// Configuration interactive du LLM et des paramètres.
    Config {
        /// Affiche la configuration actuelle et quitte.
        #[arg(long)]
        show: bool,
    },
}

pub fn main_inner(cli: Cli) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move { run(cli).await })
}

async fn run(_cli: Cli) -> anyhow::Result<()> {
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,souls=debug".into());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(log_filter))
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Config { show }) => {
            if *show {
                let cfg = config::load_config();
                println!("\n{}", "Configuration actuelle :".bright_cyan().bold());
                println!("{}", toml::to_string_pretty(&cfg)?);
                println!("\n{} {}", "Fichier :".bright_blue(), config_path_display());
                return Ok(());
            }

            let existing = config::load_config();
            let new_cfg = config::run_config_menu(&existing)?;
            println!(
                "\n{} Config courante pour souls :",
                "→".bright_green().bold()
            );
            println!(
                "  souls --provider {} --model {} --llm-url {}",
                new_cfg.llm.provider.bright_yellow(),
                new_cfg.llm.model.bright_yellow(),
                new_cfg.llm.base_url.bright_yellow(),
            );
            if let Some(ref key) = new_cfg.llm.api_key {
                println!(
                    "  --api-key {}...{}",
                    &key[..4.min(key.len())],
                    &key[key.len().saturating_sub(4)..]
                );
            }
            return Ok(());
        }
        Some(Command::Tui) => {
            let persisted = config::load_config();
            let provider = cli.provider;
            let llm_url = cli.llm_url.clone();
            let model = cli.model.clone();
            let api_key = cli.api_key.clone();

            let llm_cfg = LlmConfig {
                provider,
                base_url: llm_url,
                model,
                temperature: persisted.llm.temperature,
                http_timeout: Duration::from_secs(30),
                connect_timeout: Duration::from_secs(5),
                auth_token: api_key,
                max_tokens: persisted.llm.max_tokens,
                goal_token_budget: 50000,
                tokens_per_minute_budget: 100000,
                pool_max_idle: 10,
                pool_idle_timeout: Duration::from_secs(30),
            };
            let mut repl_state =
                soul_repl::ReplState::new(llm_cfg).map_err(|e| anyhow::anyhow!(e))?;
            repl_state.entity_name = cli.name.clone();
            soul_repl::run_repl(&mut repl_state)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(());
        }
        Some(Command::Run) | None => {}
    }

    // Charger config persistante, fusionner avec CLI
    let persisted = config::load_config();
    let provider = cli.provider;
    let llm_url = cli.llm_url.clone();
    let model = cli.model.clone();
    let api_key = cli.api_key.clone();
    let name = cli.name.clone();

    print_banner(
        &provider,
        &llm_url,
        &model,
        &name,
        &cli.gateway,
        &cli.metrics,
        cli.autonomous,
    );

    let telemetry_hub = Arc::new(soul_telemetry::TelemetryHub::new(num_cpus::get()));
    info!("telemetry hub initialisé pour {} cœurs", num_cpus::get());

    let sandbox_policy = if cli.strict_sandbox {
        SandboxPolicy::strict(&[
            "ls", "cat", "head", "tail", "grep", "find", "wc", "ps", "df", "free", "uname",
            "uptime", "whoami", "echo", "pwd", "date", "git", "cargo", "python3", "bash", "curl",
            "wc",
        ])
    } else {
        SandboxPolicy::default()
    };

    let entity_config = EntityConfig {
        name: name.clone(),
        llm: LlmConfig {
            provider,
            base_url: llm_url.clone(),
            model: model.clone(),
            temperature: persisted.llm.temperature,
            http_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            auth_token: api_key.clone(),
            max_tokens: persisted.llm.max_tokens,
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

    let entity = Arc::new(SoulEntity::new(entity_config).map_err(|e| anyhow::anyhow!("{e}"))?);
    info!("entité {} initialisée", name);

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

    entity.create_goal("Vérifier l'état initial du système", 5);
    info!("goal de démarrage créé");

    // ── SoulLink BrainMesh wiring ──────────────────────────────────
    let brain = Arc::new(brain::BrainMesh::new(100));
    info!(
        brain_status = %brain.status(),
        "🧠 BrainMesh wired into entity"
    );

    // Background brain maintenance every 60s
    let brain_maintenance = brain.clone();
    let _brain_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = brain_maintenance.maintain().await {
                tracing::warn!("BrainMesh maintenance failed: {e}");
            }
        }
    });
    info!("🧠 BrainMesh consolidation loop started");

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

    let metrics_addr: SocketAddr = cli
        .metrics
        .parse()
        .map_err(|e| anyhow::anyhow!("adresse metrics invalide: {e}"))?;

    let hub_for_metrics = telemetry_hub.clone();
    let exporter = Arc::new(
        soul_telemetry::PrometheusExporter::new()
            .map_err(|e| anyhow::anyhow!("création exporter Prometheus: {e}"))?,
    );
    let metrics_listener = tokio::net::TcpListener::bind(metrics_addr)
        .await
        .map_err(|e| anyhow::anyhow!("échec bind metrics server: {e}"))?;

    let metrics_handle = tokio::spawn(async move {
        let app = axum::Router::new()
            .route(
                "/metrics",
                axum::routing::get({
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
                }),
            )
            .route("/health", axum::routing::get(|| async { "OK" }));

        info!("serveur métriques Prometheus sur http://{metrics_addr}/metrics");

        if let Err(e) = axum::serve(metrics_listener, app).await {
            tracing::error!("metrics server crashed: {e}");
        }
    });

    let clinical_handle = match entity
        .subsystems
        .start_clinical_console(gw_addr.port().saturating_add(1))
    {
        Ok(h) => {
            info!("serveur clinique TCP sur port {}", gw_addr.port() + 1);
            Some(h)
        }
        Err(e) => {
            tracing::warn!("serveur clinique indisponible: {e}");
            None
        }
    };

    if cli.repl {
        info!("démarrage du REPL interactif");
        let llm_cfg = LlmConfig {
            provider,
            base_url: llm_url.clone(),
            model: model.clone(),
            temperature: persisted.llm.temperature,
            http_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            auth_token: api_key.clone(),
            max_tokens: persisted.llm.max_tokens,
            goal_token_budget: 50000,
            tokens_per_minute_budget: 100000,
            pool_max_idle: 10,
            pool_idle_timeout: Duration::from_secs(30),
        };
        let mut repl_state = soul_repl::ReplState::new(llm_cfg).map_err(|e| anyhow::anyhow!(e))?;
        repl_state.entity_name = name.clone();
        soul_repl::run_repl(&mut repl_state)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
    }

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

    println!("\n{}", "SoulSystem arrêté proprement.".bright_cyan().bold());
    let final_status = entity_for_status.status();
    println!(
        "{}",
        serde_json::to_string_pretty(&final_status).unwrap_or_default()
    );
    Ok(())
}

#[allow(dead_code)]
#[tokio::main]
async fn main() {
    if let Err(e) = main_inner(Cli::parse()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn config_path_display() -> String {
    dirs::config_dir()
        .map(|d| {
            d.join("soulsystem")
                .join("config.toml")
                .display()
                .to_string()
        })
        .unwrap_or_else(|| "~/.config/soulsystem/config.toml".into())
}

fn print_banner(
    provider: &ProviderKind,
    llm_url: &str,
    model: &str,
    name: &str,
    gateway: &str,
    metrics: &str,
    autonomous: bool,
) {
    let provider_label = match provider {
        ProviderKind::Ollama => "Ollama",
        ProviderKind::OpenAI => "OpenAI",
        ProviderKind::Anthropic => "Anthropic",
    };
    let banner = format!(
        r"
╔══════════════════════════════════════════════════════════════╗
║   SoulSystem  —  Entité Numérique Autonome                  ║
║   Provider  : {:<47} ║
║   Nom       : {:<47} ║
║   Gateway   : {:<47} ║
║   Métriques : {:<47} ║
║   URL LLM   : {:<47} ║
║   Modèle    : {:<47} ║
║   Mémoire   : {:<47} ║
║   Autonome  : {:<47} ║
╚══════════════════════════════════════════════════════════════╝
",
        provider_label,
        name,
        gateway,
        metrics,
        llm_url,
        model,
        "/tmp",
        if autonomous { "oui" } else { "non" }
    );
    println!("{}", banner.bright_blue());
}
