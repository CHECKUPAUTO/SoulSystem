//! Main evolution loop — wires together every module that used to be orphaned
//! in the previous merge: analysis, mutation_engine, pareto, repair,
//! semantic_diff, transfer, prompt_memory and fuzzer.
//!
//! Flow per iteration:
//!   1. Sample a parent (NSGA-II if enabled, else tournament).
//!   2. Build a prompt enriched with transfer-learning patterns + history.
//!   3. Mutate (structured AST first, creative LLM fallback).
//!   4. Evaluate. If score is 0, try `RepairEngine` and re-evaluate.
//!   5. Analyse the resulting code with tree-sitter → store quality metrics.
//!   6. Record outcome in `PromptMemory`, broadcast iteration event.
//!   7. Save program; periodically: migrate islands, checkpoint, optional fuzz.

use crate::analysis::analyze;
use crate::checkpoint::Checkpoint;
use crate::config::Config;
use crate::database::ProgramDatabase;
use crate::embedding::EmbeddingClient;
use crate::evaluator::Evaluator;
use crate::fuzzer::Fuzzer;
use crate::llm::LlmClient;
use crate::map_elites::MapElites;
use crate::metrics as om;
use crate::mutation_engine::MutationEngine;
use crate::ncpu_filter::{NcpuFilter, Verdict};
use crate::pareto::nsga2_select;
use crate::persistence as db;
use crate::persistence::SqlitePool;
use crate::program::Program;
use crate::prompt::PromptBuilder;
use crate::prompt_memory::PromptMemory;
use crate::repair::{RepairConfig, RepairEngine};
use crate::semantic_diff::diff_semantic;
use crate::transfer::TransferEngine;
use crate::wal::{WalEntry, WalWriter};
use anyhow::Result;
use colored::Colorize;
use rand::Rng;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

pub struct EvolutionEngine {
    cfg: Config,
    db: ProgramDatabase,
    llm: LlmClient,
    eval: Evaluator,
    prompt: PromptBuilder,
    prompt_memory: PromptMemory,
    mutator: MutationEngine,
    repair: RepairEngine,
    transfer: TransferEngine,
    fuzzer: Fuzzer,
    initial_code: String,
    output_dir: PathBuf,
    history: Vec<Program>,
    language: String,
    events: Option<broadcast::Sender<serde_json::Value>>,

    // ── v4.2 additions ──
    /// SQLite + WAL persistent storage (`None` when disabled).
    persist: Option<SqlitePool>,
    wal: Option<WalWriter>,
    run_id: String,
    embeddings: EmbeddingClient,
    ncpu: NcpuFilter,
    map_elites: MapElites,
    cost: Arc<om::CostTracker>,
}

impl EvolutionEngine {
    pub async fn new(cfg: Config, initial_code: String) -> Result<Self> {
        let db = ProgramDatabase::new(cfg.database.clone()).await?;
        let cache = LlmClient::make_cache(cfg.cache.clone());
        let llm = LlmClient::new(cfg.llm.clone())?.with_cache(cache);
        let eval = Evaluator::new(cfg.evaluator.clone());
        let output_dir = PathBuf::from(&cfg.database.output_dir);

        let prompt = PromptBuilder::new(
            "Tu es un expert. Améliore l'algorithme suivant pour maximiser \
             le score retourné par l'évaluateur. Modifie uniquement le bloc \
             entre EVOLVE-BLOCK-START et EVOLVE-BLOCK-END.",
        )
        .with_top(3)
        .with_diverse(2);

        let prompt_memory = PromptMemory::new();
        let mutator = MutationEngine::new(0.7);
        let repair = RepairEngine::new(llm.clone(), RepairConfig::default());
        let transfer = TransferEngine::new();
        let fuzzer = Fuzzer::new();
        let language = "rust".to_string();

        // ── v4.2: persistent storage ──
        let (persist, wal) = if cfg.persistence.enabled {
            let db_path = std::path::Path::new(&cfg.persistence.sqlite_path);
            let pool = db::open_pool(db_path)?;
            let wal_path = if cfg.persistence.wal_path.is_empty() {
                output_dir.join("wal.jsonl")
            } else {
                PathBuf::from(&cfg.persistence.wal_path)
            };
            let wal = WalWriter::open(&wal_path)?;
            (Some(pool), Some(wal))
        } else {
            (None, None)
        };

        let run_id = uuid::Uuid::new_v4().to_string();
        if let Some(pool) = &persist {
            let cfg_json = serde_json::to_string(&cfg).unwrap_or_else(|_| "{}".into());
            db::create_run(pool, &run_id, &cfg_json)?;
        }

        let embeddings = EmbeddingClient::new(cfg.embedding.clone());
        let ncpu = NcpuFilter::new(cfg.ncpu.clone());
        let map_elites = MapElites::new();
        let cost = Arc::new(om::CostTracker::new());

        // Best-effort metrics init.
        let _ = om::init();

        Ok(Self {
            cfg,
            db,
            llm,
            eval,
            prompt,
            prompt_memory,
            mutator,
            repair,
            transfer,
            fuzzer,
            initial_code,
            output_dir,
            history: Vec::new(),
            language,
            events: None,
            persist,
            wal,
            run_id,
            embeddings,
            ncpu,
            map_elites,
            cost,
        })
    }

    /// Retourne le meilleur programme de la population.
    pub async fn best(&self) -> Result<Option<Program>> {
        Ok(self.db.best().await)
    }

    /// Run ID — useful for cross-referencing in the dashboard.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Expose the cost tracker so the server can show live spend.
    pub fn cost_tracker(&self) -> Arc<om::CostTracker> {
        Arc::clone(&self.cost)
    }

    /// Override the source language (default is "rust").
    pub fn set_language(&mut self, lang: impl Into<String>) {
        self.language = lang.into();
    }

    /// Plug a broadcast channel so server-mode can stream iteration events.
    pub fn set_event_tx(&mut self, tx: broadcast::Sender<serde_json::Value>) {
        self.events = Some(tx);
    }

    pub async fn resume_from(&mut self, checkpoint_dir: &Path) -> Result<()> {
        let ckpt = Checkpoint::load(checkpoint_dir).await?;
        info!(
            iteration = ckpt.iteration,
            n = ckpt.programs.len(),
            "resuming from checkpoint"
        );
        for p in ckpt.programs {
            self.history.push(p.clone());
            self.db.add(p).await?;
        }
        Ok(())
    }

    pub async fn run(&mut self, max_iter: u32) -> Result<()> {
        println!("{}", "=== OpenEvolve v4.1 ===".bold().cyan());
        println!("Iterations     : {max_iter}");
        println!(
            "Iles           : {}  |  Population max : {}",
            self.cfg.database.num_islands, self.cfg.database.population_size
        );
        println!(
            "LLM principal  : {}  ({:.0}%)",
            self.cfg.llm.primary_model,
            self.cfg.llm.primary_weight * 100.0
        );
        println!();

        // ── Seed evaluation ───────────────────────────────────────────────
        let mut seed = Program::new(self.initial_code.clone(), 0, None, 0);
        self.eval.evaluate(&mut seed).await?;
        self.annotate_with_analysis(&mut seed);
        println!("{} Score initial : {:.4}", ">".green(), seed.score);
        self.history.push(seed.clone());
        self.db.add(seed).await?;
        self.emit_event(0, None, "seed");

        let mut last_feedback: Option<String> = None;

        for iter in 1..=max_iter {
            // ── Parent selection ─────────────────────────────────────────
            let Some(parent) = self.pick_parent().await else {
                warn!("population empty, skipping iter {iter}");
                continue;
            };

            // ── Build prompt enriched with transfer + memory ─────────────
            let complexity_class = parent
                .metrics
                .get("complexity_class")
                .map(|f| (*f as u32).to_string())
                .unwrap_or_else(|| "unknown".into());
            let base_prompt =
                self.prompt
                    .build_mutation_prompt(&parent, &self.history, last_feedback.as_deref());
            let enriched = self
                .transfer
                .build_prompt_with_transfer(&base_prompt, &parent.code, &self.language, 5)
                .unwrap_or(base_prompt);
            let full_prompt =
                self.prompt_memory
                    .select_prompt(&self.language, &complexity_class, 0.3, &enriched);

            // ── Mutate ──────────────────────────────────────────────────
            let (new_code, op) = match self
                .mutator
                .mutate_via_prompt(&parent, &self.language, &full_prompt, &self.llm)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(error=%e, iter, "mutation failed");
                    continue;
                }
            };
            if new_code.trim().is_empty() {
                warn!(iter, "empty code returned");
                om::inc_iteration("failed");
                continue;
            }

            // ── nCPU pre-filter (sub-millisecond reject of obviously-bad code) ──
            if self.ncpu.classify(&new_code, &self.language).await == Verdict::Reject {
                info!(iter, "nCPU rejected candidate, skipping eval");
                om::inc_iteration("skipped");
                om::record_evaluation("ncpu_reject");
                continue;
            }

            // ── Evaluate, repair on failure ─────────────────────────────
            let island_id = rand::thread_rng().gen_range(0..self.cfg.database.num_islands.max(1));
            let mut child = Program::new(
                new_code.clone(),
                parent.generation + 1,
                Some(parent.id.clone()),
                island_id,
            );
            child.language = Some(self.language.clone());
            child.provenance = Some(op.name().to_string());
            let eval_start = std::time::Instant::now();
            self.eval.evaluate(&mut child).await?;
            om::record_evaluation(if child.score > 0.0 { "ok" } else { "zero" });

            if child.score == 0.0 {
                let err = child
                    .metrics
                    .get("error")
                    .map(|f| format!("{f}"))
                    .unwrap_or_else(|| "evaluation returned score 0".into());
                if let Some(fixed) = self.repair.repair(&child.code, &self.language, &err).await {
                    info!(iter, "repair succeeded, re-evaluating");
                    child.code = fixed;
                    self.eval.evaluate(&mut child).await?;
                }
            }

            // ── Static analysis on the resulting code ───────────────────
            self.annotate_with_analysis(&mut child);

            // ── Semantic diff for traceability ──────────────────────────
            let changes = diff_semantic(&parent.code, &child.code);
            if !changes.is_empty() {
                let labels: Vec<&str> = changes.iter().map(|c| c.label()).collect();
                info!(iter, ?labels, "semantic changes detected");
            }

            // ── Record prompt outcome ───────────────────────────────────
            self.prompt_memory
                .record(&full_prompt, &self.language, child.score);

            // ── Optional feedback from secondary every 5 iterations ─────
            if child.score < 0.95 && iter % 5 == 0 {
                let fb_prompt =
                    PromptBuilder::build_feedback_prompt(&child.code, child.score, &child.metrics);
                if let Ok(fb) = self.llm.feedback_with_prompt(&fb_prompt).await {
                    last_feedback = Some(fb);
                }
            }

            // ── Logging ────────────────────────────────────────────────
            let delta = child.score - parent.score;
            // Adaptive mutation rate: feed back the outcome so the engine drifts
            // toward whichever path is currently producing improvements.
            let was_structured = op != crate::mutation_engine::MutationOp::Creative;
            self.mutator.note_outcome(was_structured, delta > 0.0);
            let delta_str = if delta >= 0.0 {
                format!("+{delta:.4}").green().to_string()
            } else {
                format!("{delta:.4}").red().to_string()
            };
            println!(
                "[{iter:>4}/{max_iter}] Ile {island_id} | Score {:.4} ({delta_str}) | Gen {} | op={}",
                child.score,
                child.generation,
                op.name()
            );

            self.emit_event(iter, Some(&child), op.name());

            // ── v4.2: Prometheus + WAL + SQLite + Map-Elites ────────────
            let elapsed = eval_start.elapsed().as_secs_f64();
            om::record_iteration_duration(elapsed);
            om::inc_mutation(op.name());
            om::inc_iteration("ok");
            // Best score gauge (consider history + new child)
            let best_so_far = self
                .history
                .iter()
                .map(|p| p.score)
                .fold(child.score, f64::max);
            om::set_best_score(best_so_far);

            // Compute embedding for the child (TRIBE if configured, else local).
            let emb = self.embeddings.embed(&child.code).await;

            // SQLite persistence
            if let Some(pool) = &self.persist {
                if let Err(e) = db::insert_program(pool, &self.run_id, &child, Some(&emb)) {
                    warn!(error=%e, "sqlite insert failed");
                }
            }
            // WAL append
            if let Some(wal) = &self.wal {
                let _ = wal.append(&WalEntry {
                    run_id: self.run_id.clone(),
                    iteration: iter,
                    ts: chrono::Utc::now(),
                    op: op.name().to_string(),
                    score: Some(child.score),
                    payload: serde_json::json!({
                        "program_id": child.id,
                        "parent_id": child.parent_id,
                        "island": child.island_id,
                        "generation": child.generation,
                    }),
                });
            }
            // Map-Elites archive
            if self.cfg.map_elites.enabled {
                let accepted = self.map_elites.insert(child.clone());
                if accepted {
                    info!(
                        cells = self.map_elites.len(),
                        "map-elites accepted candidate"
                    );
                }
            }
            // Cost guard
            if self.cfg.cost.max_usd > 0.0 {
                let spent = self.cost.total_usd();
                if spent >= self.cfg.cost.max_usd {
                    warn!(
                        spent_usd = spent,
                        ceiling = self.cfg.cost.max_usd,
                        "cost ceiling reached, aborting run"
                    );
                    break;
                }
                if spent >= self.cfg.cost.max_usd * self.cfg.cost.warn_fraction {
                    warn!(spent_usd = spent, "cost warn threshold crossed");
                }
            }

            self.history.push(child.clone());
            if self.history.len() > 50 {
                self.history.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                self.history.truncate(30);
            }

            self.db.add(child).await?;

            // ── Periodic maintenance ────────────────────────────────────
            if iter % self.cfg.database.migration_interval == 0 {
                self.db.migrate().await?;
            }
            if iter % self.cfg.evolution.checkpoint_interval == 0 {
                self.save_checkpoint(iter).await?;
                println!("{}", format!("  -> checkpoint {iter} saved").dimmed());
            }
        }

        // ── Final wrap-up ──────────────────────────────────────────────
        self.save_checkpoint(max_iter).await?;
        self.db.save_best(max_iter).await?;

        // Quick fuzz pass on the top program (smoke test).
        if let Some(best) = self.db.best().await {
            let inputs = self.fuzzer.fuzz_inputs(20);
            info!(
                n_inputs = inputs.len(),
                best_score = best.score,
                "fuzz inputs generated (run via evaluator out-of-band)"
            );
        }

        if let Some(best) = self.db.best().await {
            println!();
            println!("{}", "=== Resultat final ===".bold().cyan());
            println!("Meilleur score  : {:.4}", best.score);
            println!("Generation      : {}", best.generation);
            for (k, v) in &best.metrics {
                println!("  {k:<24}: {v:.4}");
            }
            println!(
                "Repair success  : {:.2} %",
                self.repair.stats().success_rate() * 100.0
            );
            println!(
                "Prompt avg score: {:.4}",
                self.prompt_memory.success_rate(&self.language)
            );
            println!(
                "Sauvegarde dans : {}/best/best_program.rs",
                self.cfg.database.output_dir
            );
        }
        Ok(())
    }

    // ── helpers ─────────────────────────────────────────────────────────

    async fn pick_parent(&self) -> Option<Program> {
        // If multi-objective metrics are present in enough programs, use NSGA-II.
        if self.cfg.evolution.log_level != "off" && self.history.len() >= 4 {
            // Run NSGA-II over the rolling history to break ties via crowding.
            let n = self.history.len().min(8);
            let selected = nsga2_select(&self.history, n);
            if !selected.is_empty() {
                let mut rng = rand::thread_rng();
                let pick = selected[rng.gen_range(0..selected.len())];
                if let Some(p) = self.history.get(pick) {
                    return Some(p.clone());
                }
            }
        }
        self.db.sample_parent().await
    }

    fn annotate_with_analysis(&self, program: &mut Program) {
        let res = analyze(&program.code, &self.language);
        program
            .metrics
            .insert("static_quality".into(), res.quality_score);
        program
            .metrics
            .insert("cyclomatic".into(), res.cyclomatic_complexity as f64);
        program
            .metrics
            .insert("nesting".into(), res.max_nesting_depth as f64);
        program.metrics.insert("loc".into(), res.loc as f64);
        program
            .metrics
            .insert("function_count".into(), res.function_count as f64);
        program.metrics.insert(
            "security_warnings".into(),
            res.security_warnings.len() as f64,
        );
        program
            .metrics
            .insert("unused_imports".into(), res.unused_imports.len() as f64);
        // Encode complexity class as an integer
        program.metrics.insert(
            "complexity_class".into(),
            match res.complexity_class {
                crate::analysis::ComplexityClass::Constant => 0.0,
                crate::analysis::ComplexityClass::Log => 1.0,
                crate::analysis::ComplexityClass::Linear => 2.0,
                crate::analysis::ComplexityClass::Linearithmic => 3.0,
                crate::analysis::ComplexityClass::Quadratic => 4.0,
                crate::analysis::ComplexityClass::CubicPlus => 5.0,
                crate::analysis::ComplexityClass::Exponential => 6.0,
                crate::analysis::ComplexityClass::Factorial => 7.0,
            },
        );
    }

    fn emit_event(&self, iteration: u32, child: Option<&Program>, op: &str) {
        if let Some(tx) = &self.events {
            let payload = serde_json::json!({
                "iteration": iteration,
                "op": op,
                "score": child.map(|c| c.score),
                "best_score": self.history.iter().map(|p| p.score).fold(f64::MIN, f64::max),
                "status": if iteration == 0 { "seed" } else { "running" },
            });
            let _ = tx.send(payload);
        }
    }

    async fn save_checkpoint(&self, iteration: u32) -> Result<()> {
        let ckpt = Checkpoint::new(iteration, self.history.clone());
        ckpt.save(&self.output_dir).await?;
        Ok(())
    }
}
