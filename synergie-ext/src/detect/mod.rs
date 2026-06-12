//! Détecteurs de synergies — orchestration parallèle (rayon).
//!
//! Les détecteurs sync s'exécutent en parallèle via rayon ; le détecteur
//! sémantique est async (appel TRIBE via `EmbedClient`).
//!
//! Compat ascendante : `DetectContext` conserve `projects`, `cfg`,
//! `expressions` (utilisés par `symbolic.rs`), et ajoute des champs
//! optionnels pour les détecteurs v3.

use crate::scanner::{FileIndex, ProjectDeps, RuntimeSnapshot, PublicItem, CoCommitMatrix};
use crate::types::{Project, Synergy};
use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub mod symbolic;
pub mod duplicate;
pub mod semantic;
pub mod deps_graph;
pub mod api_surface;
pub mod config_drift;
pub mod port_overlap;
pub mod cocommit;
pub mod lang_bridge;
pub mod doc_xref;

/// Contexte partagé passé à chaque détecteur.
///
/// Les champs historiques (`projects`, `cfg`, `expressions`) sont conservés
/// pour compat avec `symbolic.rs`. Les champs v3 sont ajoutés.
#[derive(Clone)]
pub struct DetectContext {
    // ── historique (conservé tel quel) ──
    pub projects: Vec<ProjectInfo>,
    pub cfg: DetectConfig,
    pub expressions: Vec<ProjectExpressions>,

    // ── v3 ──
    pub ecosystem: Arc<Vec<Project>>,
    pub file_index: Arc<HashMap<String, FileIndex>>,
    pub public_items: Arc<HashMap<String, Vec<PublicItem>>>,
    pub deps: Arc<HashMap<String, ProjectDeps>>,
    pub runtime: Arc<Option<RuntimeSnapshot>>,
    pub cocommits: Arc<Option<CoCommitMatrix>>,
}

impl DetectContext {
    /// Constructeur "vide" pour les tests/legacy.
    pub fn legacy(projects: Vec<ProjectInfo>, expressions: Vec<ProjectExpressions>) -> Self {
        Self {
            projects,
            cfg: DetectConfig::default(),
            expressions,
            ecosystem: Arc::new(vec![]),
            file_index: Arc::new(HashMap::new()),
            public_items: Arc::new(HashMap::new()),
            deps: Arc::new(HashMap::new()),
            runtime: Arc::new(None),
            cocommits: Arc::new(None),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectConfig {
    /// Symbolique (existant).
    pub symbolic: bool,

    /// MinHash + Jaccard.
    pub duplicate: bool,
    /// Embeddings TRIBE local.
    pub semantic: bool,
    /// Dépendances partagées.
    pub deps_graph: bool,
    /// Chaînes producer/consumer.
    pub api_surface: bool,
    /// Clés TOML/YAML/.env communes.
    pub config_drift: bool,
    /// Ports identiques/voisins.
    pub port_overlap: bool,
    /// Co-commits git.
    pub cocommit: bool,
    /// Rust ↔ Python.
    pub lang_bridge: bool,
    /// READMEs croisés.
    pub doc_xref: bool,

    // Paramètres fins.
    #[serde(default = "default_minhash_signatures")]
    pub minhash_signatures: usize,
    #[serde(default = "default_duplicate_jaccard")]
    pub duplicate_jaccard_min: f32,
    #[serde(default = "default_semantic_threshold")]
    pub semantic_threshold: f32,
    #[serde(default = "default_cocommit_window")]
    pub cocommit_window: usize,
}

fn default_minhash_signatures() -> usize { 128 }
fn default_duplicate_jaccard() -> f32 { 0.75 }
fn default_semantic_threshold() -> f32 { 0.86 }
fn default_cocommit_window() -> usize { 200 }

impl Default for DetectConfig {
    fn default() -> Self {
        Self {
            symbolic: true,
            duplicate: true,
            semantic: true,
            deps_graph: true,
            api_surface: true,
            config_drift: true,
            port_overlap: true,
            cocommit: true,
            lang_bridge: true,
            doc_xref: true,
            minhash_signatures: 128,
            duplicate_jaccard_min: 0.75,
            semantic_threshold: 0.86,
            cocommit_window: 200,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectExpressions {
    pub project_name: String,
    pub expressions: Vec<String>,
}

/// Trait commun à tous les détecteurs sync.
pub trait Detector: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>>;
}

/// Lance les détecteurs sync activés en parallèle via rayon.
///
/// Le détecteur sémantique étant async, il est lancé séparément (cf. `run_semantic`).
pub fn run_sync_detectors(ctx: &DetectContext) -> Result<Vec<Synergy>> {
    let mut detectors: Vec<Box<dyn Detector>> = Vec::new();

    if ctx.cfg.symbolic {
        detectors.push(Box::new(symbolic::SymbolicDetector));
    }
    if ctx.cfg.duplicate {
        detectors.push(Box::new(duplicate::DuplicateDetector::default()));
    }
    if ctx.cfg.deps_graph {
        detectors.push(Box::new(deps_graph::DepsGraphDetector::default()));
    }
    if ctx.cfg.api_surface {
        detectors.push(Box::new(api_surface::ApiSurfaceDetector::default()));
    }
    if ctx.cfg.config_drift {
        detectors.push(Box::new(config_drift::ConfigDriftDetector::default()));
    }
    if ctx.cfg.port_overlap {
        detectors.push(Box::new(port_overlap::PortOverlapDetector::default()));
    }
    if ctx.cfg.cocommit {
        detectors.push(Box::new(cocommit::CoCommitDetector::default()));
    }
    if ctx.cfg.lang_bridge {
        detectors.push(Box::new(lang_bridge::LangBridgeDetector::default()));
    }
    if ctx.cfg.doc_xref {
        detectors.push(Box::new(doc_xref::DocXrefDetector::default()));
    }

    info!(target: "detect", n = detectors.len(), "détecteurs sync");

    let results: Vec<Vec<Synergy>> = detectors
        .par_iter()
        .map(|d| {
            let t0 = std::time::Instant::now();
            match d.detect(ctx) {
                Ok(v) => {
                    debug!(target: "detect", det = d.name(), found = v.len(), ms = t0.elapsed().as_millis(), "ok");
                    v
                }
                Err(e) => {
                    warn!(target: "detect", det = d.name(), error = %e, "détecteur en erreur");
                    Vec::new()
                }
            }
        })
        .collect();

    Ok(results.into_iter().flatten().collect())
}

/// Lance le détecteur sémantique (async).
pub async fn run_semantic(ctx: &DetectContext, embed: &mut crate::embed::EmbedClient) -> Result<Vec<Synergy>> {
    if !ctx.cfg.semantic {
        return Ok(vec![]);
    }
    semantic::run(ctx, embed).await
}

/// Dédoublonne par `id` ; conserve la meilleure `auto_score` et fusionne les evidence.
pub fn merge_synergies(all: Vec<Synergy>) -> Vec<Synergy> {
    let mut by_id: HashMap<String, Synergy> = HashMap::new();
    for s in all {
        match by_id.get_mut(&s.id) {
            None => {
                by_id.insert(s.id.clone(), s);
            }
            Some(existing) => {
                if s.auto_score > existing.auto_score {
                    let mut keep = s;
                    keep.evidence.extend(existing.evidence.drain(..));
                    *existing = keep;
                } else {
                    let mut seen: std::collections::HashSet<String> =
                        existing.evidence.iter().map(|e| e.fragment.clone()).collect();
                    for e in s.evidence {
                        if seen.insert(e.fragment.clone()) {
                            existing.evidence.push(e);
                        }
                    }
                }
            }
        }
    }
    by_id.into_values().collect()
}
