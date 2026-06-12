//! Synergie v3 — Agent autonome de détection de synergies dans l'écosystème.
//!
//! Sous-commandes :
//!   scan     — un seul scan, écrit le rapport et sort
//!   daemon   — boucle infinie + actions automatiques (Telegram/GitHub)
//!   serve    — démarre HTTP/WS + tick daemon (équivalent à daemon + serve)
//!   mcp      — serveur MCP stdio (JSON-RPC 2.0)
//!   report   — affiche le dernier rapport sauvegardé
//!   inspect  — affiche l'écosystème détecté (projets/manifests)
//!   sync     — push GitHub des proposals
//!   status   — affiche la config + compteurs DB

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod agent;
mod analyzer;
mod api;
mod config;
mod detect;
mod ecosystem;
mod embed;
mod filter;
mod github;
mod mcp;
mod memory;
mod reporter;
mod rstdp;
mod scanner;
mod telegram;
mod types;

#[derive(Parser, Debug)]
#[command(
    name = "synergie",
    version,
    about = "Agent autonome de détection de synergies",
    long_about = None,
)]
struct Cli {
    /// Chemin du TOML (défaut: $SYNERGIE_CONFIG, /etc/synergie/synergie.toml, ./synergie.toml).
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Un seul scan, écrit le rapport et sort.
    Scan {
        /// Liste de détecteurs (csv): symbolic,duplicate,semantic,deps,api,config,ports,cocommit,bridge,docs
        #[arg(long)]
        only: Option<String>,
        /// Ignore le cache d'embeddings.
        #[arg(long)]
        no_cache: bool,
    },
    /// Boucle daemon (tick + actions auto).
    Daemon,
    /// HTTP/WS API + boucle daemon.
    Serve {
        #[arg(long)]
        port: Option<u16>,
    },
    /// Serveur MCP stdio.
    Mcp,
    /// Affiche le dernier rapport.
    Report,
    /// Affiche l'écosystème détecté.
    Inspect,
    /// Push GitHub des proposals.
    Sync,
    /// Affiche la config + compteurs DB.
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,synergie=info")))
        .init();

    let cli = Cli::parse();
    let cfg = match cli.config.as_ref() {
        Some(p) => config::Config::load(p)?,
        None => config::Config::try_load_default(),
    };
    let cfg = Arc::new(cfg);
    let agent = Arc::new(agent::Agent::new(cfg.clone()));

    match cli.cmd {
        Cmd::Scan { only, no_cache } => {
            if no_cache {
                if let Ok(cache) = memory::embedding_cache() {
                    let _ = cache.clear();
                }
            }
            let parsed: Option<HashSet<String>> = only.map(|s| {
                s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
            });
            let report = agent.one_shot_scan(parsed.as_ref()).await?;
            let path = agent.write_report(&report).await?;
            println!("{}", reporter::summary_string(&report));
            println!("Rapport écrit : {}", path.display());
        }
        Cmd::Daemon => {
            agent.clone().run_forever().await?;
        }
        Cmd::Serve { port } => {
            let mut cfg_mut = (*cfg).clone();
            if let Some(p) = port { cfg_mut.api.port = p; }
            let cfg_arc = Arc::new(cfg_mut);
            let agent2 = Arc::new(agent::Agent::new(cfg_arc.clone()));
            let agent_loop = agent2.clone();
            tokio::spawn(async move {
                if let Err(e) = agent_loop.run_forever().await {
                    tracing::error!("daemon loop crashed: {}", e);
                }
            });
            api::serve(cfg_arc, agent2).await?;
        }
        Cmd::Mcp => {
            mcp::run_stdio(agent).await?;
        }
        Cmd::Report => {
            let r = agent.load_last_report().await?;
            println!("{}", reporter::summary_string(&r));
            for s in r.synergies.iter().take(20) {
                println!(
                    "  [{:.2}] {:>4} {} ({}) {} → {}",
                    s.adjusted_score, s.priority, s.synergy_type, s.id, s.source_project, s.target_project
                );
            }
        }
        Cmd::Inspect => {
            let eco = ecosystem::Ecosystem::discover(&cfg)?;
            println!("Découverte de {} projet(s) :", eco.projects.len());
            for p in eco.projects {
                println!("  • {:<35} {:?}  [{}]", p.name, p.kind, p.tags.join(","));
                for m in &p.manifests { println!("      ↳ {}", m.display()); }
                for s in &p.services { println!("      ↳ service {} port={:?} ({})", s.kind, s.port, s.source); }
            }
        }
        Cmd::Sync => {
            let Some(repo) = &cfg.action.github_proposals_repo else {
                anyhow::bail!("aucun repo proposals configuré (action.github_proposals_repo)");
            };
            let remote = cfg.action.github_remote.clone().unwrap_or_else(|| "origin".into());
            github::push(repo, &remote, "main")?;
            info!("push GitHub OK");
        }
        Cmd::Status => {
            let counts = memory::counts().unwrap_or(memory::MemoryCounts {
                synergies: 0, embed_cache: 0, history: 0, feedback: 0,
            });
            let opt = memory::load_optimizer().ok();
            println!("SYNERGIE v{}", env!("CARGO_PKG_VERSION"));
            println!("Tick : {} s  |  Threshold : {:.2}  |  Dry run : {}", cfg.agent.tick_seconds, cfg.agent.action_threshold, cfg.agent.dry_run);
            println!("API  : http://{}:{}", cfg.api.bind, cfg.api.port);
            println!("DB   : {} / {} synergies, {} hist, {} feedback, {} embeds",
                cfg.persistence.path.display(),
                counts.synergies, counts.history, counts.feedback, counts.embed_cache);
            if let Some(o) = opt {
                println!("R-STDP : lr={} momentum={} | {} coefficients", o.lr(), o.momentum(), o.coefficients().len());
                for (k, v) in o.coefficients() {
                    println!("   {:<24} = {:.3}", k, v);
                }
            }
        }
    }
    Ok(())
}
