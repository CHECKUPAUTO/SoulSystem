//! OpenEvolve CLI — uses the `openevolve` library.
//!
//! Subcommands:
//!   - `evolve`   (default) : run an evolution session
//!   - `server`              : start the REST + WebSocket monitoring server
//!   - `diagnose`            : run system + source diagnostics
//!   - `analyze <file>`      : tree-sitter analysis report for one file
//!   - `library`             : list saved programs

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use openevolve::{
    analysis::analyze,
    checkpoint::Checkpoint,
    config::Config,
    database::ProgramDatabase,
    diagnostic::{print_report, SystemDiagnostic},
    evolution::EvolutionEngine,
    server::run_server,
};
use std::path::PathBuf;

/// OpenEvolve v4.3 — Evolutionary code optimization via LLM (100% Rust)
#[derive(Parser, Debug)]
#[command(
    name = "openevolve",
    version = "4.3.0",
    about,
    long_about = "Evolves source code with an LLM-guided genetic algorithm."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Initial source file (default subcommand only)
    #[arg(
        short = 'p',
        long,
        default_value = "examples/my_task/src/solution.rs",
        global = true
    )]
    program: PathBuf,

    /// Evaluator binary (default subcommand only)
    #[arg(short = 'e', long, default_value = "./evaluator", global = true)]
    evaluator: PathBuf,

    /// Iteration count (default subcommand)
    #[arg(short = 'i', long, global = true)]
    iterations: Option<u32>,

    /// TOML config file
    #[arg(short = 'c', long, global = true)]
    config: Option<PathBuf>,

    /// Resume from a checkpoint directory
    #[arg(long, global = true)]
    resume: Option<PathBuf>,

    /// Quick mode (uses config.evolution.quick_iterations)
    #[arg(long, global = true)]
    quick: bool,

    /// Prerequisites check (no run)
    #[arg(long, global = true)]
    check: bool,

    /// Print default config as TOML, then exit
    #[arg(long, global = true)]
    print_config: bool,

    /// List checkpoints, then exit
    #[arg(long, global = true)]
    list_checkpoints: bool,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run an evolution session (default behaviour)
    Evolve,
    /// Start REST + WebSocket monitoring server
    Server {
        #[arg(long, default_value = "127.0.0.1:8460")]
        bind: String,
    },
    /// Run system / source diagnostics
    Diagnose {
        #[arg(long, default_value = "src")]
        source_dir: String,
    },
    /// Tree-sitter analysis of a single source file
    Analyze {
        file: PathBuf,
        #[arg(long, default_value = "rust")]
        language: String,
    },
    /// List the most recent saved programs from the run output directory
    Library {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// (v4.3) List the runs recorded in the SQLite database
    Runs {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// (v4.3) Replay an evolution session from the JSONL WAL file
    ReplayWal {
        path: PathBuf,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// (v4.3) Export top-N programs of a run to a JSON file
    DbExport {
        run_id: String,
        #[arg(long, default_value = "export.json")]
        out: PathBuf,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// (v4.3) Discover `// @evolve` annotations in a file or directory
    EvolveTag {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openevolve=info".parse().unwrap()),
        )
        .init();

    if cli.print_config {
        println!("{}", Config::default_config().to_toml()?);
        return Ok(());
    }
    if cli.check {
        return check_prerequisites(&cli);
    }

    let cfg = load_config(&cli)?;

    if cli.list_checkpoints {
        return list_checkpoints(&cfg).await;
    }

    match cli.cmd {
        None | Some(Cmd::Evolve) => cmd_evolve(cli, cfg).await,
        Some(Cmd::Server { bind }) => cmd_server(cfg, bind).await,
        Some(Cmd::Diagnose { source_dir }) => cmd_diagnose(&source_dir),
        Some(Cmd::Analyze { file, language }) => cmd_analyze(&file, &language),
        Some(Cmd::Library { limit }) => cmd_library(&cfg, limit).await,
        Some(Cmd::Runs { limit }) => cmd_runs(&cfg, limit),
        Some(Cmd::ReplayWal { path, limit }) => cmd_replay_wal(&path, limit),
        Some(Cmd::DbExport { run_id, out, limit }) => cmd_db_export(&cfg, &run_id, &out, limit),
        Some(Cmd::EvolveTag { path, json }) => cmd_evolve_tag(&path, json),
    }
}

fn load_config(cli: &Cli) -> Result<Config> {
    let mut cfg = if let Some(ref p) = cli.config {
        Config::from_file(p)?
    } else {
        let default_path = PathBuf::from("configs/default.toml");
        if default_path.exists() {
            Config::from_file(&default_path)?
        } else {
            Config::default_config()
        }
    };
    if cli.evaluator.to_str().unwrap_or("") != "./evaluator" {
        cfg.evaluator.evaluator_bin = cli.evaluator.display().to_string();
    }
    Ok(cfg)
}

async fn cmd_evolve(cli: Cli, cfg: Config) -> Result<()> {
    let max_iter = if cli.quick {
        cfg.evolution.quick_iterations
    } else {
        cli.iterations.unwrap_or(cfg.evolution.max_iterations)
    };
    let initial_code = std::fs::read_to_string(&cli.program)
        .with_context(|| format!("read {}", cli.program.display()))?;
    let mut engine = EvolutionEngine::new(cfg, initial_code).await?;
    if let Some(dir) = cli.resume.as_ref() {
        engine.resume_from(dir).await?;
    }
    engine.run(max_iter).await
}

async fn cmd_server(cfg: Config, bind: String) -> Result<()> {
    let db = ProgramDatabase::new(cfg.database.clone()).await?;
    run_server(cfg, db, &bind).await
}

fn cmd_diagnose(source_dir: &str) -> Result<()> {
    let diag = SystemDiagnostic::new();
    let report = diag.run_full(source_dir);
    print_report(&report);
    Ok(())
}

fn cmd_analyze(file: &std::path::Path, language: &str) -> Result<()> {
    let code = std::fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
    let r = analyze(&code, language);
    println!(
        "{}",
        "── Static analysis ────────────────────────────"
            .bold()
            .cyan()
    );
    println!("File          : {}", file.display());
    println!("Language      : {language}");
    println!("LOC           : {}", r.loc);
    println!("Functions     : {}", r.function_count);
    println!("Cyclomatic    : {}", r.cyclomatic_complexity);
    println!("Max nesting   : {}", r.max_nesting_depth);
    println!("Complexity    : {}", r.complexity_class.label());
    println!("Quality       : {:.4}", r.quality_score);
    if !r.penalties.is_empty() {
        println!("Penalties     : {}", r.penalties.join(", "));
    }
    if !r.security_warnings.is_empty() {
        println!("Security      : {}", r.security_warnings.join("; "));
    }
    if !r.unused_imports.is_empty() {
        println!("Unused imports: {}", r.unused_imports.join(", "));
    }
    if !r.unused_variables.is_empty() {
        println!("Unused vars   : {}", r.unused_variables.join(", "));
    }
    if !r.long_functions.is_empty() {
        println!("Long funcs    : {}", r.long_functions.join("; "));
    }
    Ok(())
}

async fn cmd_library(cfg: &Config, limit: usize) -> Result<()> {
    let prog_dir = PathBuf::from(&cfg.database.output_dir).join("programs");
    if !prog_dir.exists() {
        println!("No programs directory at {}", prog_dir.display());
        return Ok(());
    }
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&prog_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            Some((m, e.path()))
        })
        .collect();
    entries.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    println!(
        "{}",
        format!("── {} most recent programs ──", limit)
            .bold()
            .cyan()
    );
    for (_, path) in entries.into_iter().take(limit) {
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(p) = serde_json::from_str::<openevolve::program::Program>(&s) {
                println!(
                    "{:>7.4}  gen={:>3} island={} {}",
                    p.score, p.generation, p.island_id, p.id
                );
            }
        }
    }
    Ok(())
}

fn check_prerequisites(cli: &Cli) -> Result<()> {
    println!("{}", "Vérification des prérequis…".bold());
    let checks = [
        (
            "Programme initial",
            cli.program.exists(),
            cli.program.display().to_string(),
        ),
        (
            "Évaluateur",
            cli.evaluator.exists(),
            cli.evaluator.display().to_string(),
        ),
    ];
    let mut all_ok = true;
    for (label, ok, path) in &checks {
        println!(
            "  {} {:<20} {}",
            if *ok { "✔".green() } else { "✘".red() },
            label,
            path
        );
        if !ok {
            all_ok = false;
        }
    }
    let ollama_ok = std::process::Command::new("curl")
        .args(["-sf", "http://localhost:11434/api/tags"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!(
        "  {} {:<20} port 11434",
        if ollama_ok {
            "✔".green()
        } else {
            "✘".red()
        },
        "Ollama"
    );
    if !ollama_ok {
        all_ok = false;
    }
    println!();
    println!(
        "{}",
        if all_ok {
            "✔ Tout est prêt !".green().bold().to_string()
        } else {
            "✘ Prérequis manquants.".red().to_string()
        }
    );
    Ok(())
}

async fn list_checkpoints(cfg: &Config) -> Result<()> {
    let output_dir = PathBuf::from(&cfg.database.output_dir);
    let cps = Checkpoint::list(&output_dir).await?;
    if cps.is_empty() {
        println!("Aucun checkpoint trouvé dans {}", output_dir.display());
    } else {
        println!("{} checkpoints dans {}:", cps.len(), output_dir.display());
        for ckpt in &cps {
            println!("  • {}", ckpt.display());
        }
        println!(
            "\nReprise : openevolve --resume {}",
            cps.last().unwrap().display()
        );
    }
    Ok(())
}

// ── v4.3 subcommands ────────────────────────────────────────────────────

fn cmd_runs(cfg: &Config, limit: usize) -> Result<()> {
    let pool =
        openevolve::persistence::open_pool(std::path::Path::new(&cfg.persistence.sqlite_path))?;
    let runs = openevolve::persistence::list_runs(&pool, limit)?;
    if runs.is_empty() {
        println!("Aucun run trouvé dans {}", cfg.persistence.sqlite_path);
        return Ok(());
    }
    println!(
        "{}",
        format!(
            "── {} run(s) — {} ──",
            runs.len(),
            cfg.persistence.sqlite_path
        )
        .bold()
        .cyan()
    );
    for r in &runs {
        let count = openevolve::persistence::count_programs(&pool, &r.id).unwrap_or(0);
        let best = openevolve::persistence::best_program(&pool, &r.id)
            .ok()
            .flatten()
            .map(|p| p.score)
            .unwrap_or(0.0);
        let (ti, to, usd) =
            openevolve::persistence::run_total_cost(&pool, &r.id).unwrap_or((0, 0, 0.0));
        println!(
            "  {} {:<10} progs={:<5} best={:.4} tok={}/{} ${:.4} started={}",
            r.id.chars().take(8).collect::<String>(),
            r.status,
            count,
            best,
            ti,
            to,
            usd,
            &r.started_at[..19.min(r.started_at.len())],
        );
    }
    Ok(())
}

fn cmd_replay_wal(path: &std::path::Path, limit: usize) -> Result<()> {
    let entries = openevolve::wal::load(path)?;
    let total = entries.len();
    println!(
        "{}",
        format!("── WAL {} ({} entries) ──", path.display(), total)
            .bold()
            .cyan()
    );
    for e in entries.iter().take(limit) {
        let score = e
            .score
            .map(|s| format!("{:.4}", s))
            .unwrap_or_else(|| "—".into());
        println!(
            "  iter={:>5} op={:<10} score={:<8} ts={}",
            e.iteration,
            e.op,
            score,
            &e.ts.to_rfc3339()[..19],
        );
    }
    if total > limit {
        println!("  … {} more (use --limit to see them)", total - limit);
    }
    Ok(())
}

fn cmd_db_export(cfg: &Config, run_id: &str, out: &std::path::Path, limit: usize) -> Result<()> {
    let pool =
        openevolve::persistence::open_pool(std::path::Path::new(&cfg.persistence.sqlite_path))?;
    let progs = openevolve::persistence::top_programs(&pool, run_id, limit)?;
    if progs.is_empty() {
        println!("Aucun programme pour run_id={run_id}");
        return Ok(());
    }
    let json = serde_json::to_string_pretty(&progs)?;
    std::fs::write(out, json).with_context(|| format!("write {}", out.display()))?;
    println!(
        "{} {} programmes exportés vers {}",
        "✔".green(),
        progs.len(),
        out.display()
    );
    Ok(())
}

fn cmd_evolve_tag(path: &std::path::Path, json: bool) -> Result<()> {
    let tags = if path.is_dir() {
        openevolve::evolve_tag::parse_directory(path)?
    } else {
        openevolve::evolve_tag::parse_file(path)?
    };
    if tags.is_empty() {
        println!(
            "Aucune annotation `// @evolve` trouvée dans {}",
            path.display()
        );
        return Ok(());
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&tags)?);
    } else {
        println!(
            "{}",
            format!("── {} @evolve tag(s) — {} ──", tags.len(), path.display())
                .bold()
                .cyan()
        );
        for t in &tags {
            println!(
                "  {}:{} fn={} fitness={} iter={} lang={}",
                t.source_file.display(),
                t.line,
                t.function_name.as_deref().unwrap_or("—"),
                t.fitness.display(),
                t.iterations
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "—".into()),
                t.language.as_deref().unwrap_or("—"),
            );
        }
    }
    Ok(())
}
