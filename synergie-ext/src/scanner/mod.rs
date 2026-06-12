//! Couche perception : indexation FS, AST, dépendances, runtime, git.
//!
//! Conserve `SynergyScanner` pour la compatibilité ascendante avec
//! l'ancien `main.rs`, mais les détecteurs v3 consomment plutôt les types
//! re-exportés ici (`FileIndex`, `PublicItem`, `ProjectDeps`,
//! `RuntimeSnapshot`, `CoCommitMatrix`).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

pub mod deps;
pub mod fs;
pub mod git;
pub mod python_ast;
pub mod runtime;
pub mod rust_ast;

// ─── Re-exports pour les détecteurs ─────────────────────────────────────

pub use deps::{Dependency, ProjectDeps};
pub use fs::FileIndex;
pub use git::CoCommitMatrix;
pub use runtime::{ListeningPort, RuntimeSnapshot, SystemdUnit};

// ─── Types AST communs ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Fn,
    AsyncFn,
    Struct,
    Enum,
    Trait,
    Impl,
    Const,
    Static,
    Module,
    PyFunction,
    PyClass,
    PyMethod,
}

impl ItemKind {
    pub fn is_callable(&self) -> bool {
        matches!(
            self,
            ItemKind::Fn | ItemKind::AsyncFn | ItemKind::PyFunction | ItemKind::PyMethod
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::AsyncFn => "async fn",
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Trait => "trait",
            ItemKind::Impl => "impl",
            ItemKind::Const => "const",
            ItemKind::Static => "static",
            ItemKind::Module => "mod",
            ItemKind::PyFunction => "def",
            ItemKind::PyClass => "class",
            ItemKind::PyMethod => "method",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicItem {
    pub project: String,
    pub file: PathBuf,
    pub line: usize,
    pub kind: ItemKind,
    pub name: String,
    pub signature: String,
    #[serde(default)]
    pub body_tokens: Vec<String>,
    pub doc: Option<String>,
}

impl PublicItem {
    pub fn kind_label(&self) -> &'static str {
        self.kind.label()
    }
}

// ─── SynergyScanner — compat main.rs original ──────────────────────────

pub struct SynergyScanner {
    output_dir: PathBuf,
}

impl SynergyScanner {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    pub async fn scan_all(&self) -> Result<()> {
        info!(target: "scanner", out = %self.output_dir.display(), "scan ecosystem (legacy entrypoint)");
        std::fs::create_dir_all(&self.output_dir).ok();
        Ok(())
    }

    pub fn output_dir(&self) -> &PathBuf {
        &self.output_dir
    }
}
