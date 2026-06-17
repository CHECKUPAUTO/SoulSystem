//! Agent autonome : orchestration du cycle complet.
//!
//! Cycle d'un tick :
//!   1. Découverte écosystème
//!   2. Indexation FS par projet (rayon)
//!   3. AST Rust + Python par projet (rayon)
//!   4. Deps par projet (rayon)
//!   5. Snapshot runtime (ss/systemctl/proc)
//!   6. Matrices de co-commits (git, fusionnées par racine de repo)
//!   7. Détecteurs sync en parallèle (rayon)
//!   8. Détecteur sémantique async (embeddings TRIBE/local)
//!   9. Filtres (symboliques + dédup)
//!  10. Re-score R-STDP
//!  11. Upsert sled + mark_unseen_stale
//!  12. Build report + sauvegarde JSON/MD + Telegram + Github scaffolding

use crate::config::Config;
use crate::detect::{self, DetectContext};
use crate::ecosystem::Ecosystem;
use crate::embed::EmbedClient;
use crate::scanner::{self, FileIndex, ProjectDeps, PublicItem};
use crate::types::{Language, Project, ScanReport, Synergy};
use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Événements diffusés sur le bus broadcast (lus par WS et MCP).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    AgentTick {
        tick: u64,
        ts: chrono::DateTime<chrono::Utc>,
    },
    ScanCompleted {
        synergies: usize,
        elapsed_ms: u64,
    },
    SynergyDetected {
        id: String,
        score: f32,
        title: String,
    },
    SynergyUpdated {
        id: String,
        status: String,
    },
    ActionApplied {
        synergy_id: String,
        kind: String,
        success: bool,
    },
    Log {
        level: String,
        msg: String,
    },
}

pub struct Agent {
    cfg: Arc<Config>,
    events_tx: broadcast::Sender<Event>,
    tick_counter: AtomicU64,
}

impl Agent {
    pub fn new(cfg: Arc<Config>) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            cfg,
            events_tx: tx,
            tick_counter: AtomicU64::new(0),
        }
    }

    pub fn cfg(&self) -> &Config {
        &self.cfg
    }
    pub fn events_tx(&self) -> &broadcast::Sender<Event> {
        &self.events_tx
    }

    fn emit(&self, ev: Event) {
        let _ = self.events_tx.send(ev);
    }

    /// Cycle complet — un seul tour.
    pub async fn one_shot_scan(&self, only: Option<&HashSet<String>>) -> Result<ScanReport> {
        let t0 = Instant::now();
        let tick = self.tick_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.emit(Event::AgentTick {
            tick,
            ts: chrono::Utc::now(),
        });

        // 1) Découverte
        let eco = Ecosystem::discover(&self.cfg)?;
        let projects = eco.projects;
        info!(target: "agent", n_projects = projects.len(), "écosystème découvert");

        // 2) Indexation FS
        let file_index: HashMap<String, FileIndex> = projects
            .par_iter()
            .map(|p| (p.name.clone(), scanner::fs::index(p, &self.cfg.ecosystem)))
            .collect();

        // 3) AST Rust + Python
        let public_items: HashMap<String, Vec<PublicItem>> = projects
            .par_iter()
            .map(|p| {
                let idx = match file_index.get(&p.name) {
                    Some(i) => i,
                    None => return (p.name.clone(), Vec::new()),
                };
                let mut items = Vec::new();
                let rs_files = idx.files_of(Language::Rust);
                if !rs_files.is_empty() {
                    match scanner::rust_ast::parse_project(&p.name, rs_files) {
                        Ok(v) => items.extend(v),
                        Err(e) => warn!(target: "agent.ast", project = %p.name, error = %e),
                    }
                }
                let py_files = idx.files_of(Language::Python);
                if !py_files.is_empty() {
                    match scanner::python_ast::parse_project(&p.name, py_files) {
                        Ok(v) => items.extend(v),
                        Err(e) => warn!(target: "agent.ast", project = %p.name, error = %e),
                    }
                }
                (p.name.clone(), items)
            })
            .collect();
        let total_items: usize = public_items.values().map(|v| v.len()).sum();
        info!(target: "agent", items = total_items, "items publics indexés");

        // 4) Deps
        let deps: HashMap<String, ProjectDeps> = projects
            .par_iter()
            .map(|p| (p.name.clone(), scanner::deps::scan_project(p)))
            .collect();

        // 5) Runtime
        let runtime = match scanner::runtime::snapshot() {
            Ok(r) => Some(r),
            Err(e) => {
                warn!(target: "agent.runtime", error = %e);
                None
            }
        };

        // 6) Co-commits
        let mut cocommits = scanner::CoCommitMatrix::default();
        let mut seen_roots: HashSet<PathBuf> = HashSet::new();
        for p in &projects {
            let mut cur = p.path.clone();
            loop {
                if cur.join(".git").is_dir() {
                    if seen_roots.insert(cur.clone()) {
                        match scanner::git::analyze_repo(&cur, self.cfg.agent.git_window) {
                            Ok(m) => cocommits.merge(m),
                            Err(e) => {
                                debug!(target: "agent.git", path = %cur.display(), error = %e)
                            }
                        }
                    }
                    break;
                }
                match cur.parent() {
                    Some(parent) if parent != cur.as_path() => cur = parent.to_path_buf(),
                    _ => break,
                }
            }
        }

        // 7) Contexte de détection
        let ctx = DetectContext {
            projects: legacy_project_infos(&projects, &file_index),
            cfg: filter_cfg(&self.cfg.detectors, only),
            expressions: vec![],
            ecosystem: Arc::new(projects.clone()),
            file_index: Arc::new(file_index),
            public_items: Arc::new(public_items),
            deps: Arc::new(deps),
            runtime: Arc::new(runtime),
            cocommits: Arc::new(if cocommits.is_empty() {
                None
            } else {
                Some(cocommits)
            }),
        };

        // 8) Détecteurs sync (parallèle)
        let sync_synergies = detect::run_sync_detectors(&ctx).unwrap_or_default();
        info!(target: "agent.detect", n = sync_synergies.len(), "détecteurs sync");

        // 9) Détecteur sémantique async
        let mut semantic_synergies = Vec::new();
        if self.cfg.embed.enabled && ctx.cfg.semantic {
            let cache = match crate::memory::embedding_cache() {
                Ok(c) => c,
                Err(e) => {
                    warn!(target: "agent.embed", error = %e);
                    return self
                        .finalize(
                            t0,
                            ctx.ecosystem.as_ref().clone(),
                            sync_synergies,
                            ctx.file_index.as_ref(),
                        )
                        .await;
                }
            };
            let mut embed = match EmbedClient::new(self.cfg.embed.clone(), cache) {
                Ok(c) => c,
                Err(e) => {
                    warn!(target: "agent.embed", error = %e, "embed client KO");
                    return self
                        .finalize(
                            t0,
                            ctx.ecosystem.as_ref().clone(),
                            sync_synergies,
                            ctx.file_index.as_ref(),
                        )
                        .await;
                }
            };
            match detect::run_semantic(&ctx, &mut embed).await {
                Ok(v) => semantic_synergies = v,
                Err(e) => warn!(target: "agent.detect", error = %e, "semantic KO"),
            }
        }

        // 10) Merge + filtres + score R-STDP
        let mut all = sync_synergies;
        all.extend(semantic_synergies);
        let merged = detect::merge_synergies(all);
        let mut filtered = crate::filter::apply_all(merged);
        let opt = crate::memory::load_optimizer()
            .unwrap_or_else(|_| crate::rstdp::RSTDPOptimizer::new(0.1, 0.9));
        crate::rstdp::rescore_batch(&mut filtered, &opt);

        // 11) Persistance + stale
        let seen_ids: HashSet<String> = filtered.iter().map(|s| s.id.clone()).collect();
        let now = chrono::Utc::now();
        for s in filtered.iter_mut() {
            let mut up = s.clone();
            // Préserver le created_at si la synergie existait déjà.
            if let Ok(Some(prev)) = crate::memory::get_synergy(&s.id) {
                up.created_at = prev.created_at;
                up.status = prev.status;
            }
            up.last_seen = now;
            *s = up.clone();
            if let Err(e) = crate::memory::upsert_synergy(s) {
                warn!(target: "agent.memory", error = %e);
            }
            self.emit(Event::SynergyDetected {
                id: s.id.clone(),
                score: s.adjusted_score,
                title: s.description.lines().next().unwrap_or("").to_string(),
            });
        }
        let staled = crate::memory::mark_unseen_stale(&seen_ids, self.cfg.agent.stale_after_days)
            .unwrap_or(0);
        if staled > 0 {
            info!(target: "agent", staled, "synergies marquées Stale");
        }

        // 12) Report
        let stats = crate::reporter::build_stats(&filtered);
        let elapsed_ms = t0.elapsed().as_millis() as u64;
        let mut languages: HashMap<String, usize> = HashMap::new();
        for idx in ctx.file_index.values() {
            for (lang, files) in &idx.by_lang {
                *languages.entry(format!("{:?}", lang)).or_default() += files.len();
            }
        }
        let report = ScanReport {
            generated_at: now,
            elapsed_ms,
            projects: ctx.ecosystem.as_ref().clone(),
            synergies: filtered,
            stats,
            languages,
        };
        self.emit(Event::ScanCompleted {
            synergies: report.synergies.len(),
            elapsed_ms: report.elapsed_ms,
        });
        Ok(report)
    }

    async fn finalize(
        &self,
        t0: Instant,
        projects: Vec<Project>,
        synergies: Vec<Synergy>,
        file_index: &HashMap<String, FileIndex>,
    ) -> Result<ScanReport> {
        let stats = crate::reporter::build_stats(&synergies);
        let mut languages: HashMap<String, usize> = HashMap::new();
        for idx in file_index.values() {
            for (lang, files) in &idx.by_lang {
                *languages.entry(format!("{:?}", lang)).or_default() += files.len();
            }
        }
        Ok(ScanReport {
            generated_at: chrono::Utc::now(),
            elapsed_ms: t0.elapsed().as_millis() as u64,
            projects,
            synergies,
            stats,
            languages,
        })
    }

    /// Écrit le rapport sur disque (JSON+MD+latest) et dans sled (history).
    pub async fn write_report(&self, report: &ScanReport) -> Result<PathBuf> {
        let p = crate::memory::save_report(report, &self.cfg.persistence.reports_dir)?;
        let _ = crate::memory::prune_history(self.cfg.persistence.max_history);
        Ok(p)
    }

    /// Charge le dernier rapport (depuis sled, sinon depuis la liste plate de synergies).
    pub async fn load_last_report(&self) -> Result<ScanReport> {
        match crate::memory::last_report()? {
            Some(r) => Ok(r),
            None => {
                let synergies = crate::memory::list_synergies().unwrap_or_default();
                let stats = crate::reporter::build_stats(&synergies);
                Ok(ScanReport {
                    generated_at: chrono::Utc::now(),
                    elapsed_ms: 0,
                    projects: vec![],
                    synergies,
                    stats,
                    languages: HashMap::new(),
                })
            }
        }
    }

    /// Boucle daemon : un scan tous les `tick_seconds`, action automatique
    /// selon `action_threshold` et `max_actions_per_tick`.
    pub async fn run_forever(self: Arc<Self>) -> Result<()> {
        let interval = Duration::from_secs(self.cfg.agent.tick_seconds.max(60));
        info!(target: "agent", every_s = self.cfg.agent.tick_seconds, "boucle daemon démarrée");
        loop {
            match self.one_shot_scan(None).await {
                Ok(report) => {
                    let summary = crate::reporter::summary_string(&report);
                    info!(target: "agent", "{}", summary);
                    if let Err(e) = self.write_report(&report).await {
                        warn!(target: "agent", error = %e, "écriture rapport échouée");
                    }
                    self.apply_top_actions(&report).await;
                }
                Err(e) => warn!(target: "agent", error = %e, "scan échoué"),
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn apply_top_actions(&self, report: &ScanReport) {
        let threshold = self.cfg.agent.action_threshold;
        let cap = self.cfg.agent.max_actions_per_tick;
        if cap == 0 {
            return;
        }

        let mut top: Vec<&Synergy> = report
            .synergies
            .iter()
            .filter(|s| s.adjusted_score >= threshold)
            .collect();
        top.sort_by(|a, b| {
            b.adjusted_score
                .partial_cmp(&a.adjusted_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top: Vec<&Synergy> = top.into_iter().take(cap).collect();
        if top.is_empty() {
            return;
        }

        let tele = crate::telegram::Telegram::new(
            self.cfg.action.telegram_bot_token.clone(),
            self.cfg.action.telegram_chat_id.clone(),
        );

        for syn in top {
            if self.cfg.agent.notify_on_action && tele.is_configured() {
                let _ = tele.send_synergy(syn).await;
            }
            if let Some(repo_path) = &self.cfg.action.github_proposals_repo {
                let proposals_dir = repo_path.join("proposals");
                let files = match crate::github::write_scaffolding(
                    syn,
                    &proposals_dir,
                    self.cfg.agent.dry_run,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(target: "agent.action", error = %e, "scaffolding KO");
                        self.emit(Event::ActionApplied {
                            synergy_id: syn.id.clone(),
                            kind: "scaffolding".into(),
                            success: false,
                        });
                        continue;
                    }
                };
                self.emit(Event::ActionApplied {
                    synergy_id: syn.id.clone(),
                    kind: "scaffolding".into(),
                    success: true,
                });

                if self.cfg.action.auto_apply_safe_patches && !self.cfg.agent.dry_run {
                    match crate::github::commit_proposal(
                        repo_path,
                        &self.cfg.action.branch_prefix,
                        syn,
                        &files,
                    ) {
                        Ok(sha) => {
                            info!(target: "agent.action", sha = %sha, "commit OK");
                            self.emit(Event::ActionApplied {
                                synergy_id: syn.id.clone(),
                                kind: "commit".into(),
                                success: true,
                            });
                        }
                        Err(e) => {
                            warn!(target: "agent.action", error = %e, "commit KO");
                            self.emit(Event::ActionApplied {
                                synergy_id: syn.id.clone(),
                                kind: "commit".into(),
                                success: false,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn legacy_project_infos(
    projects: &[Project],
    idx: &HashMap<String, FileIndex>,
) -> Vec<crate::detect::ProjectInfo> {
    projects
        .iter()
        .map(|p| {
            let files: Vec<String> = idx
                .get(&p.name)
                .map(|fi| {
                    fi.by_lang
                        .values()
                        .flatten()
                        .map(|pb| pb.to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            crate::detect::ProjectInfo {
                name: p.name.clone(),
                path: p.path.to_string_lossy().into_owned(),
                files,
            }
        })
        .collect()
}

fn filter_cfg(
    base: &crate::detect::DetectConfig,
    only: Option<&HashSet<String>>,
) -> crate::detect::DetectConfig {
    let mut c = base.clone();
    if let Some(set) = only {
        c.symbolic = set.contains("symbolic");
        c.duplicate = set.contains("duplicate");
        c.semantic = set.contains("semantic");
        c.deps_graph = set.contains("deps_graph") || set.contains("deps");
        c.api_surface = set.contains("api_surface") || set.contains("api");
        c.config_drift = set.contains("config_drift") || set.contains("config");
        c.port_overlap = set.contains("port_overlap") || set.contains("ports");
        c.cocommit = set.contains("cocommit");
        c.lang_bridge = set.contains("lang_bridge") || set.contains("bridge");
        c.doc_xref = set.contains("doc_xref") || set.contains("docs");
    }
    c
}

fn _ensure_path_used(_p: &Path) {}
