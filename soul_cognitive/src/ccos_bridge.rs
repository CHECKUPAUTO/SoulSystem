//! CCOS causal context integration for SoulSystem's cognitive engine.
//!
//! Wraps the CCOS causal graph engine as a drop-in memory provider.
//! The `CognitiveEngine` can use `CausalMemoryBackend` instead of the
//! flat `ContextManager` for budget-aware context windows that include
//! causally-relevant files even when the symptom file is large.

use ccos::external_memory::{CcosMemory, ExternalMemory, Recall, RecallWindow};
use ccos::memory::MemoryGraph;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Default context budget in tokens.
pub const DEFAULT_BUDGET: usize = 4096;

/// Errors from the CCOS integration layer.
#[derive(Debug, thiserror::Error)]
pub enum CausalError {
    #[error("CCOS engine error: {0}")]
    Engine(String),
    #[error("Recall failed: {0}")]
    Recall(String),
    #[error("Source not found: {0}")]
    SourceNotFound(String),
}

/// A SoulSystem memory backend backed by CCOS's causal graph.
///
/// Ingest source files one at a time, then recall budgeted context
/// windows that include causally-relevant files — the CCOS win over
/// flat dumps at the same token budget.
pub struct CausalMemoryBackend {
    inner: CcosMemory,
}

impl CausalMemoryBackend {
    /// Create a new `CausalMemoryBackend`.
    pub fn new() -> Self {
        Self {
            inner: CcosMemory::new(),
        }
    }

    /// Create from a JSON snapshot (for cross-session persistence).
    pub fn from_json(json: &str) -> Result<Self, CausalError> {
        let inner = CcosMemory::from_json(json).map_err(|e| CausalError::Engine(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Export the current state as a JSON snapshot.
    pub fn to_json(&self) -> Result<String, CausalError> {
        self.inner
            .to_json()
            .map_err(|e| CausalError::Engine(e.to_string()))
    }

    /// Ingest a single source file into the causal graph.
    ///
    /// `uri` is the file path (e.g. `src/config.rs`), `source` is the
    /// source code text.
    pub fn ingest_source(&mut self, uri: &str, source: &str) {
        self.inner.ingest_source(uri, source);
    }

    /// Recursively collect all `.rs` files under a directory.
    fn collect_rs_files(path: &Path) -> Vec<(String, String)> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    files.extend(Self::collect_rs_files(&p));
                } else if p.extension().is_some_and(|e| e == "rs") {
                    if let Ok(source) = std::fs::read_to_string(&p) {
                        let rel = p
                            .strip_prefix(path)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .to_string();
                        files.push((rel, source));
                    }
                }
            }
        }
        files
    }

    /// Ingest all `.rs` files under a directory.
    pub fn ingest_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), CausalError> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(CausalError::SourceNotFound(path.display().to_string()));
        }
        for (uri, source) in Self::collect_rs_files(path) {
            self.inner.ingest_source(&uri, &source);
        }
        Ok(())
    }

    /// Recall a budgeted context window around a file.
    ///
    /// Returns the CCOS recall window containing causally-relevant
    /// items scored and ordered by relevance, up to `budget` tokens.
    pub fn recall(&self, failing_file: &str, budget: usize) -> RecallWindow {
        self.inner.recall(&Recall::around(failing_file), budget)
    }

    /// Recall and assemble a flat context string from causally-relevant items.
    ///
    /// This is the main integration point: SoulSystem's LLM pipeline
    /// gets a budgeted window that includes causally-linked files even
    /// when a naive dump would truncate them.
    pub fn recall_text(&self, failing_file: &str, budget: usize) -> CausalContextWindow {
        let window = self.recall(failing_file, budget);
        let mut text = String::new();
        for item in &window.items {
            if !item.content.is_empty() {
                text.push_str(&format!(
                    "// {} [score={:.3}][kind={}]\n{}\n\n",
                    item.uri, item.score, item.kind, item.content
                ));
            }
        }
        CausalContextWindow {
            text,
            tokens: window.tokens,
            item_count: window.items.len(),
        }
    }

    /// Get the current causal graph (for debugging / inspection).
    pub fn graph(&self) -> &MemoryGraph {
        self.inner.graph()
    }
}

impl Default for CausalMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// A budgeted context window assembled by CCOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalContextWindow {
    /// The context text assembled by CCOS.
    pub text: String,
    /// Estimated token count.
    pub tokens: usize,
    /// Number of source items in the window.
    pub item_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn setup_project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ccos_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("lib.rs"),
            "pub mod config;\npub fn run() -> u8 { config::get() }\n",
        )
        .unwrap();
        fs::write(src.join("config.rs"), "pub fn get() -> u8 { 0 }\n").unwrap();
        dir
    }

    #[test]
    fn test_causal_memory_cycle() {
        let dir = setup_project();
        let src = dir.join("src");

        let mut mem = CausalMemoryBackend::new();
        mem.ingest_dir(&src).unwrap();

        // ingest_dir stores files relative to the src directory (e.g. "lib.rs", "config.rs")
        let ctx = mem.recall_text("lib.rs", 1024);
        eprintln!(
            "RECALL TEXT ({} tokens, {} items):\n{}",
            ctx.tokens, ctx.item_count, ctx.text
        );
        // After ingest_dir, URIs are "lib.rs" and "config.rs" (relative to src/)
        // recall expects "lib.rs" not "src/lib.rs"
        assert!(ctx.item_count > 0, "should have items");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_json_roundtrip() {
        let dir = setup_project();
        let src = dir.join("src");

        let mut mem = CausalMemoryBackend::new();
        mem.ingest_dir(&src).unwrap();

        let json = mem.to_json().unwrap();
        let restored = CausalMemoryBackend::from_json(&json).unwrap();
        assert_eq!(restored.graph().node_count(), mem.graph().node_count());

        let _ = fs::remove_dir_all(&dir);
    }
}

// ═══════════════════════════════════════════════════════════════════
// CONTEXT SHIELD — Virtual File System for Agent Context Windows
// ═══════════════════════════════════════════════════════════════════

/// The Context Shield protects the agent's context window from overflow
/// by treating files as virtualised, topologically-indexed documents.
///
/// **Parse** → structural skeleton (anchors only, no function bodies).
/// **Page-fault** → load full body of a specific anchor on demand.
///
/// Use `ShieldViewer::skeleton()` to get the compressed view (~120 tokens
/// for a 277-line test suite instead of ~10k raw tokens).
pub struct ShieldViewer;

impl ShieldViewer {
    /// Parse a file and return its structural skeleton — the compressed
    /// view the agent should use instead of reading the whole file.
    pub fn skeleton(path: &str) -> Result<String, String> {
        let s = ccos::shield::Shield::parse(path).map_err(|e| e.to_string())?;
        Ok(s.text)
    }

    /// Page-fault: retrieve the full implementation body of a specific
    /// function/test/anchor. Call this ONLY when the agent needs to
    /// inspect or modify that specific code.
    pub fn page_fault(path: &str, anchor: &str) -> Result<String, String> {
        ccos::shield::Shield::page_fault(path, anchor).map_err(|e| e.to_string())
    }

    /// Parse a Rust source string directly and return the skeleton.
    pub fn skeleton_str(source: &str, name: &str) -> Result<String, String> {
        let s = ccos::shield::Shield::parse_str(source, name).map_err(|e| e.to_string())?;
        Ok(s.text)
    }

    /// Page-fault on an in-memory source string.
    pub fn page_fault_str(source: &str, anchor: &str) -> Result<String, String> {
        ccos::shield::Shield::page_fault_str(source, anchor).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod shield_tests {
    use super::*;

    #[test]
    fn test_shield_skeleton() {
        let source =
            "\npub fn add(x: i32) -> i32 { x + 1 }\n\npub struct Point { x: i32, y: i32 }\n";
        let sk = ShieldViewer::skeleton_str(source, "test.rs").unwrap();
        assert!(sk.contains("add"), "should find fn anchor");
        assert!(sk.contains("Point"), "should find struct anchor");
        assert!(sk.len() < 500, "skeleton should be compact");
    }

    #[test]
    fn test_shield_page_fault() {
        let source = "\nfn secret() -> i32 {\n    let x = 42;\n    x * 2\n}\n";
        let body = ShieldViewer::page_fault_str(source, "secret").unwrap();
        assert!(body.contains("let x = 42"), "should load function body");
    }
}
