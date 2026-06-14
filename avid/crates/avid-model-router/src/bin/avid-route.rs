//! `avid-route` — command-line interface to the learned, cost-aware router.
//!
//! Examples:
//! ```text
//! # Which model would handle this query, and why?
//! avid-route route "summarise this thread" --cap summarization
//! avid-route route "prove why the cluster deadlocks" --cap analysis --json
//!
//! # Inspect the difficulty features for a query.
//! avid-route explain "design a partition-tolerant plan" --cap planning --cap analysis
//!
//! # Train the difficulty model on logged outcomes, then evaluate it.
//! avid-route train --data outcomes.jsonl --out params.json --epochs 500
//! avid-route eval  --data heldout.jsonl  --params params.json
//!
//! # Set the deferral operating point (≈30% of queries go to a strong model).
//! avid-route calibrate --data queries.jsonl --target-fraction 0.30 --out params.json
//! ```
//!
//! Training / evaluation data is JSON Lines, one record per line:
//! `{"query": "...", "capabilities": ["analysis"], "label": 1.0}`
//! (`label` = 1.0 means a strong model was needed; omit it for `route`/`explain`).

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use avid_model_router::{
    learned::QueryFeatures, Capability, CostAwareRouter, ModelRegistry, RouterParams,
};
use clap::{Parser, Subcommand};
use serde::Deserialize;

#[derive(Parser)]
#[command(
    name = "avid-route",
    about = "Learned, cost-aware LLM routing — pick the cheapest model that clears the predicted bar",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Route a single query and explain the decision.
    Route {
        /// The query / prompt text.
        query: String,
        /// Required capability (repeatable), e.g. `--cap analysis --cap planning`.
        #[arg(long = "cap", value_name = "CAP")]
        caps: Vec<String>,
        /// Override the cost/quality trade-off in [0,1] (higher = cheaper).
        #[arg(long)]
        cost_aversion: Option<f64>,
        /// Router params JSON (defaults to built-in priors).
        #[arg(long)]
        params: Option<PathBuf>,
        /// Models JSON fleet (defaults to the embedded fleet).
        #[arg(long)]
        models: Option<PathBuf>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show the difficulty features and prediction for a query.
    Explain {
        query: String,
        #[arg(long = "cap", value_name = "CAP")]
        caps: Vec<String>,
        #[arg(long)]
        params: Option<PathBuf>,
    },
    /// Train the difficulty model on labelled JSONL outcomes.
    Train {
        /// JSONL file: {"query","capabilities":[...],"label":0|1} per line.
        #[arg(long)]
        data: PathBuf,
        /// Warm-start params JSON (defaults to built-in priors).
        #[arg(long)]
        params: Option<PathBuf>,
        /// Where to write the trained params JSON.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 500)]
        epochs: usize,
        #[arg(long, default_value_t = 0.3)]
        lr: f64,
    },
    /// Score the router offline on labelled JSONL.
    Eval {
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        params: Option<PathBuf>,
        #[arg(long)]
        models: Option<PathBuf>,
    },
    /// Calibrate the threshold to a target strong-model fraction.
    Calibrate {
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 0.3)]
        target_fraction: f64,
        #[arg(long)]
        params: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
}

/// One JSONL data record.
#[derive(Debug, Deserialize)]
struct Sample {
    query: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    label: Option<f64>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("avid-route: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Route {
            query,
            caps,
            cost_aversion,
            params,
            models,
            json,
        } => {
            let caps = parse_caps(&caps)?;
            let mut router = build_router(params.as_ref(), models.as_ref())?;
            if let Some(ca) = cost_aversion {
                let mut p = router.params().clone();
                p.cost_aversion = ca.clamp(0.0, 1.0);
                router.set_params(p);
            }
            let decision = router.route(&query, &caps).ok_or_else(|| {
                "no model in the fleet satisfies the requested capabilities".to_string()
            })?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "model": decision.profile.name,
                        "provider": decision.profile.provider,
                        "difficulty": decision.difficulty,
                        "target_quality": decision.target_quality,
                        "uncertain": decision.uncertain,
                        "cost_per_1k_tokens": decision.profile.cost_per_1k_tokens,
                        "reason": decision.reason,
                    })
                );
            } else {
                println!(
                    "→ {} ({})",
                    decision.profile.name, decision.profile.provider
                );
                println!("  {}", decision.reason);
            }
            Ok(())
        }

        Command::Explain {
            query,
            caps,
            params,
        } => {
            let caps = parse_caps(&caps)?;
            let model = load_params(params.as_ref())?.model;
            let features = QueryFeatures::extract(&query, &caps);
            let names = [
                "length",
                "code",
                "reasoning",
                "multistep",
                "clauses",
                "cap_hardness",
                "cap_count",
                "vocabulary",
            ];
            println!("features for: {query:?}");
            for (name, value) in names.iter().zip(features.0.iter()) {
                println!("  {name:<12} {value:.3}");
            }
            println!(
                "difficulty (P[needs strong]) = {:.3}",
                model.predict(&features)
            );
            Ok(())
        }

        Command::Train {
            data,
            params,
            out,
            epochs,
            lr,
        } => {
            let samples = load_samples(&data)?;
            let training: Vec<(QueryFeatures, f64)> = samples
                .iter()
                .map(|s| {
                    let caps = parse_caps(&s.capabilities)?;
                    Ok((
                        QueryFeatures::extract(&s.query, &caps),
                        s.label.unwrap_or(0.0),
                    ))
                })
                .collect::<Result<_, String>>()?;
            if training.is_empty() {
                return Err("no training samples found".into());
            }
            let mut p = load_params(params.as_ref())?;
            let before = p.model.log_loss(&training);
            let after = p.model.train(&training, epochs, lr);
            write_params(&out, &p)?;
            println!(
                "trained on {} samples for {epochs} epochs (lr={lr}): log-loss {before:.4} → {after:.4}",
                training.len()
            );
            println!("wrote params → {}", out.display());
            Ok(())
        }

        Command::Eval {
            data,
            params,
            models,
        } => {
            let samples = load_samples(&data)?;
            let router = build_router(params.as_ref(), models.as_ref())?;
            let eval: Vec<(String, Vec<Capability>, f64)> = samples
                .iter()
                .map(|s| {
                    let caps = parse_caps(&s.capabilities)?;
                    Ok((s.query.clone(), caps, s.label.unwrap_or(0.0)))
                })
                .collect::<Result<_, String>>()?;
            let refs: Vec<(&str, Vec<Capability>, f64)> = eval
                .iter()
                .map(|(q, c, l)| (q.as_str(), c.clone(), *l))
                .collect();
            let m = router.evaluate(&refs);
            println!("samples         {}", m.samples);
            println!("accuracy        {:.3}", m.accuracy);
            println!("strong_fraction {:.3}", m.strong_fraction);
            println!("avg_cost        {:.3}", m.avg_cost);
            Ok(())
        }

        Command::Calibrate {
            data,
            target_fraction,
            params,
            out,
        } => {
            let samples = load_samples(&data)?;
            let mut router = build_router(params.as_ref(), None)?;
            let queries: Vec<(String, Vec<Capability>)> = samples
                .iter()
                .map(|s| Ok((s.query.clone(), parse_caps(&s.capabilities)?)))
                .collect::<Result<_, String>>()?;
            let refs: Vec<(&str, Vec<Capability>)> = queries
                .iter()
                .map(|(q, c)| (q.as_str(), c.clone()))
                .collect();
            router.calibrate_threshold(&refs, target_fraction);
            write_params(&out, router.params())?;
            println!(
                "calibrated threshold to {:.4} for target strong-fraction {target_fraction}",
                router.params().threshold
            );
            println!("wrote params → {}", out.display());
            Ok(())
        }
    }
}

fn parse_caps(raw: &[String]) -> Result<Vec<Capability>, String> {
    raw.iter().map(|c| Capability::from_str(c)).collect()
}

fn load_params(path: Option<&PathBuf>) -> Result<RouterParams, String> {
    match path {
        None => Ok(RouterParams::default()),
        Some(p) => {
            let text =
                std::fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("parse params {}: {e}", p.display()))
        }
    }
}

fn write_params(path: &PathBuf, params: &RouterParams) -> Result<(), String> {
    let text = serde_json::to_string_pretty(params).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn build_router(
    params: Option<&PathBuf>,
    models: Option<&PathBuf>,
) -> Result<CostAwareRouter, String> {
    let registry = match models {
        None => ModelRegistry::with_defaults(),
        Some(p) => {
            let mut r = ModelRegistry::new();
            r.load_from_file(p)
                .map_err(|e| format!("load models {}: {e}", p.display()))?;
            r
        }
    };
    Ok(CostAwareRouter::new(registry, load_params(params)?))
}

fn load_samples(path: &PathBuf) -> Result<Vec<Sample>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let s: Sample =
            serde_json::from_str(line).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?;
        out.push(s);
    }
    Ok(out)
}
