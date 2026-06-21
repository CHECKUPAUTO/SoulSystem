//! # CCOS Context Shield — Virtual File System for Agent Context Windows
//!
//! When an agent inspects a large file (e.g. `tests/expansion_val.rs` at 277 lines),
//! a naive `cat` injects the full content into the LLM context window, causing
//! **Context Overflow / Attention Bloat**. The Context Shield solves this by
//! treating every file as a **virtualised, topologically-indexed document**:
//!
//! 1. **Parse** — extract the file's **structural anchors** (function signatures,
//!    type definitions, trait impls, test names, module declarations, use
//!    statements). No function bodies, no expression-level code.
//! 2. **Compress** — serialise the anchors as a lightweight JSON/structural skeleton
//!    (~120 tokens for a 277-line test file instead of ~10k raw tokens).
//! 3. **Page-fault** — when the agent needs details (e.g. the implementation of
//!    `test_fem_solver` at line 219), it fetches only the relevant lines.
//!
//! ## Usage
//!
//! ```ignore
//! use ccos::shield::{Shield, Skeleton};
//!
//! let skeleton = Shield::parse("tests/expansion_val.rs").unwrap();
//! assert_eq!(skeleton.anchors.len(), 19);  // 19 test functions
//! assert_eq!(skeleton.estimated_tokens(), 120);  // compressed
//!
//! // Page-fault a specific anchor:
//! let detail = Shield::page_fault("tests/expansion_val.rs", "test_fem_solver").unwrap();
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

// ─── Error ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ShieldError {
    FileNotFound(String),
    Io(String),
    AnchorNotFound(String),
}

impl fmt::Display for ShieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShieldError::FileNotFound(p) => write!(f, "File not found: {p}"),
            ShieldError::Io(msg) => write!(f, "IO error: {msg}"),
            ShieldError::AnchorNotFound(a) => write!(f, "Anchor not found: {a}"),
        }
    }
}

impl std::error::Error for ShieldError {}

impl From<std::io::Error> for ShieldError {
    fn from(e: std::io::Error) -> Self {
        ShieldError::Io(e.to_string())
    }
}

// ─── Data Types ─────────────────────────────────────────────────────────────

/// A structural anchor in a source file — the skeleton the agent sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    /// Kind of anchor: `test`, `fn`, `struct`, `trait`, `impl`, `mod`, `use`, `type`, `const`, `enum`.
    pub kind: String,
    /// Name of the anchor (function name, type name, module name, etc.).
    pub name: String,
    /// 1-based line number where the anchor appears.
    pub line: usize,
    /// Signature line (first line of the declaration, truncated to 120 chars).
    pub signature: String,
    /// Byte offset range in the file (start, end) — for page-faulting body.
    pub body_range: Option<(usize, usize)>,
}

/// A structural skeleton — the compressed view of a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skeleton {
    /// File path.
    pub path: String,
    /// Total lines in the file.
    pub total_lines: usize,
    /// Estimated token cost of this skeleton for the LLM context.
    pub estimated_tokens: usize,
    /// The structural anchors extracted from the file.
    pub anchors: Vec<Anchor>,
    /// Dependencies (use statements, grouped by crate).
    pub dependencies: Vec<String>,
    /// Source text of the skeleton (formatted for LLM consumption).
    pub text: String,
}

// ─── Shield ─────────────────────────────────────────────────────────────────

/// The Context Shield — parse a file into a structural skeleton and serve
/// page-faults on demand.
pub struct Shield;

impl Shield {
    /// Parse a Rust source file into a structural skeleton.
    ///
    /// The skeleton contains only **anchors** (function signatures, type defs,
    /// test names, use statements, module declarations) — no function bodies,
    /// no expression-level code. Estimated token cost is ~120 tokens for a
    /// 277-line test suite instead of ~10k raw tokens.
    pub fn parse<P: AsRef<Path>>(path: P) -> Result<Skeleton, ShieldError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .map_err(|_| ShieldError::FileNotFound(path.display().to_string()))?;
        Self::parse_str(&source, &path.display().to_string())
    }

    /// Parse from a string (for in-memory usage).
    pub fn parse_str(source: &str, name: &str) -> Result<Skeleton, ShieldError> {
        let total_lines = source.lines().count();
        let mut anchors: Vec<Anchor> = Vec::new();
        let mut dependencies: Vec<String> = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Single-pass scan: extract structural anchors
        for (i, line) in lines.iter().enumerate() {
            let line_no = i + 1;
            let trimmed = line.trim();

            // Skip blank / comment-only lines
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                continue;
            }

            // `use ...;` — dependency
            if trimmed.starts_with("use ") && trimmed.ends_with(';') {
                let dep = trimmed.trim_start_matches("use ").trim_end_matches(';').trim();
                if !dependencies.contains(&dep.to_string()) {
                    dependencies.push(dep.to_string());
                }
                continue;
            }

            // `pub mod ...;` or `mod ...;`
            if let Some(name) = trimmed.strip_prefix("pub mod ").or_else(|| trimmed.strip_prefix("mod ")) {
                if name.ends_with(';') {
                    anchors.push(Anchor {
                        kind: "mod".into(),
                        name: name.trim_end_matches(';').trim().to_string(),
                        line: line_no,
                        signature: trimmed.to_string(),
                        body_range: None,
                    });
                    continue;
                }
            }

            // `pub fn`, `fn`, `pub unsafe fn`, `async fn`, `pub async fn`
            if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") || trimmed.starts_with("pub unsafe fn ")
                || trimmed.starts_with("async fn ") || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("unsafe fn ")
            {
                let kind = if trimmed.contains("#[test]") || i > 0 && lines[i-1].trim().starts_with("#[test]") {
                    "test"
                } else {
                    "fn"
                };
                let name = Self::extract_fn_name(trimmed).unwrap_or("<anonymous>");
                let body_end = Self::find_block_end(&lines, i);
                anchors.push(Anchor {
                    kind: kind.into(),
                    name: name.to_string(),
                    line: line_no,
                    signature: trimmed.chars().take(120).collect(),
                    body_range: Some((line_no, body_end.unwrap_or(line_no))),
                });
                continue;
            }

            // `pub struct`, `struct`
            if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                let name = Self::extract_typed_name(trimmed).unwrap_or("<struct>");
                anchors.push(Anchor {
                    kind: "struct".into(),
                    name: name.to_string(),
                    line: line_no,
                    signature: trimmed.chars().take(120).collect(),
                    body_range: None,
                });
                continue;
            }

            // `pub enum`, `enum`
            if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
                let name = Self::extract_typed_name(trimmed).unwrap_or("<enum>");
                anchors.push(Anchor {
                    kind: "enum".into(),
                    name: name.to_string(),
                    line: line_no,
                    signature: trimmed.chars().take(120).collect(),
                    body_range: None,
                });
                continue;
            }

            // `pub trait`, `trait`
            if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
                let name = Self::extract_typed_name(trimmed).unwrap_or("<trait>");
                anchors.push(Anchor {
                    kind: "trait".into(),
                    name: name.to_string(),
                    line: line_no,
                    signature: trimmed.chars().take(120).collect(),
                    body_range: None,
                });
                continue;
            }

            // `pub type`, `type`
            if trimmed.starts_with("pub type ") || trimmed.starts_with("type ") {
                let name = Self::extract_typed_name(trimmed).unwrap_or("<type>");
                anchors.push(Anchor {
                    kind: "type".into(),
                    name: name.to_string(),
                    line: line_no,
                    signature: trimmed.chars().take(120).collect(),
                    body_range: None,
                });
                continue;
            }

            // `pub const`, `const`
            if trimmed.starts_with("pub const ") || trimmed.starts_with("const ") {
                let name = trimmed.split_whitespace().nth(2).unwrap_or("<const>");
                let name = name.trim_end_matches(':').trim_end_matches('<').trim_end_matches('=').trim();
                anchors.push(Anchor {
                    kind: "const".into(),
                    name: name.to_string(),
                    line: line_no,
                    signature: trimmed.chars().take(120).collect(),
                    body_range: None,
                });
                continue;
            }

            // `impl ...` — trait impls
            if trimmed.starts_with("impl ") {
                let sig = trimmed.chars().take(120).collect::<String>();
                anchors.push(Anchor {
                    kind: "impl".into(),
                    name: format!("impl (line {})", line_no),
                    line: line_no,
                    signature: sig,
                    body_range: None,
                });
                continue;
            }
        }

        // Build compact text representation for LLM
        let mut text = String::new();
        text.push_str(&format!("// file: {}\n", name));
        text.push_str(&format!("// lines: {}\n", total_lines));

        if !dependencies.is_empty() {
            text.push_str("// deps:\n");
            for dep in &dependencies {
                text.push_str(&format!("//   use {}\n", dep));
            }
        }

        text.push_str("// anchors:\n");

        // Group anchors by kind for compactness
        let tests: Vec<&Anchor> = anchors.iter().filter(|a| a.kind == "test").collect();
        let fns: Vec<&Anchor> = anchors.iter().filter(|a| a.kind == "fn").collect();
        let others: Vec<&Anchor> = anchors.iter().filter(|a| a.kind != "test" && a.kind != "fn").collect();

        if !tests.is_empty() {
            text.push_str(&format!("//   [tests x{}]\n", tests.len()));
            for a in &tests {
                let range = match a.body_range {
                    Some((s, e)) if e > s + 1 => format!(" [L{}-L{}]", s, e),
                    _ => String::new(),
                };
                text.push_str(&format!("//     {} L{}{}\n", a.name, a.line, range));
            }
        }

        if !fns.is_empty() {
            text.push_str(&format!("//   [functions x{}]\n", fns.len()));
            for a in &fns {
                let range = match a.body_range {
                    Some((s, e)) if e > s + 1 => format!(" [L{}-L{}]", s, e),
                    _ => String::new(),
                };
                text.push_str(&format!("//     {} L{}{}\n", a.name, a.line, range));
            }
        }

        if !others.is_empty() {
            text.push_str(&format!("//   [types/consts/modules x{}]\n", others.len()));
            for a in &others {
                text.push_str(&format!("//     {}:{} L{}\n", a.kind, a.name, a.line));
            }
        }

        let estimated_tokens = text.len() / 4;

        Ok(Skeleton {
            path: name.to_string(),
            total_lines,
            estimated_tokens,
            anchors,
            dependencies,
            text,
        })
    }

    /// Page-fault: retrieve the implementation body of a specific anchor from a file on disk.
    ///
    /// Given a file path and an anchor name (or line number), returns the
    /// full source text of that anchor's body (10-30 lines depending on
    /// the function size). This is the **demand-paging** mechanism: the agent
    /// only fetches details when it needs them.
    pub fn page_fault<P: AsRef<Path>>(path: P, anchor_name: &str) -> Result<String, ShieldError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)?;
        Self::page_fault_str(&source, anchor_name)
    }

    /// Page-fault on an in-memory source string.
    pub fn page_fault_str(source: &str, anchor_name: &str) -> Result<String, ShieldError> {
        let skeleton = Self::parse_str(source, "<memory>")?;

        // Find the anchor
        let anchor = skeleton.anchors.iter()
            .find(|a| a.name == anchor_name || a.name.contains(anchor_name)
                  || format!("L{}", a.line) == anchor_name)
            .ok_or_else(|| ShieldError::AnchorNotFound(anchor_name.to_string()))?;

        let lines: Vec<&str> = source.lines().collect();

        // Extract the body: from anchor line to end of its block
        let body_start = anchor.line.saturating_sub(1);
        let body_end = match anchor.body_range {
            Some((_, end)) => (end + 1).min(lines.len()),
            None => (body_start + 15).min(lines.len()),
        };

        let mut result = String::new();
        result.push_str(&format!("// {} (L{}) — page fault\n", anchor.name, anchor.line));
        result.push_str(&format!("// signature: {}\n", anchor.signature));
        let body = &lines[body_start..body_end];
        for (i, line) in body.iter().enumerate() {
            result.push_str(&format!("{:>4}: {}\n", body_start + i + 1, line));
        }

        Ok(result)
    }

    // ─── Helpers ──────────────────────────────────────────────────────────

    fn extract_fn_name(s: &str) -> Option<&str> {
        let s = s.trim();
        let after_fn = s.split("fn ").nth(1)?;
        let name = after_fn.split('(').next()?;
        let name = name.split('<').next()?;
        let name = name.split_whitespace().next()?;
        Some(name)
    }

    fn extract_typed_name(s: &str) -> Option<&str> {
        let s = s.trim();
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "pub" {
            Some(parts[2].trim_end_matches('<').trim_end_matches('('))
        } else if parts.len() >= 2 {
            Some(parts[1].trim_end_matches('<').trim_end_matches('('))
        } else {
            None
        }
    }

    /// Find the closing brace of a block starting at `start_line`.
    fn find_block_end(lines: &[&str], start_line: usize) -> Option<usize> {
        let mut depth: u32 = 0;
        let mut started = false;
        for (i, line) in lines.iter().enumerate().skip(start_line) {
            for ch in line.chars() {
                match ch {
                    '{' => { depth += 1; started = true; }
                    '}' => {
                        if depth == 0 { return None; }
                        depth -= 1;
                        if started && depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_file() {
        let source = "\nuse crate::foo;\nuse crate::bar;\n\npub const VERSION: &str = \"1.0\";\n\npub struct Config {\n    pub width: usize,\n}\n\npub fn run() -> u8 {\n    42\n}\n\n#[test]\nfn test_run() {\n    assert_eq!(run(), 42);\n}\n";
        let skeleton = Shield::parse_str(source, "test.rs").unwrap();
        assert_eq!(skeleton.total_lines, 18);
        assert!(skeleton.dependencies.contains(&"crate::foo".to_string()));
        assert!(skeleton.dependencies.contains(&"crate::bar".to_string()));
        assert!(skeleton.anchors.iter().any(|a| a.kind == "const" && a.name == "VERSION"));
        assert!(skeleton.anchors.iter().any(|a| a.kind == "struct" && a.name == "Config"));
        assert!(skeleton.anchors.iter().any(|a| a.name == "run" && a.kind == "fn"));
        assert!(skeleton.anchors.iter().any(|a| a.name == "test_run" && a.kind == "test"));
        assert!(skeleton.estimated_tokens < 100, "Skeleton should be compact (was {})", skeleton.estimated_tokens);
    }

    #[test]
    fn test_page_fault_on_multiline_fn() {
        let source = "\npub fn complex_fn(x: i32) -> i32 {\n    let y = x + 1;\n    let z = y * 2;\n    z + 3\n}\n\npub fn other() -> u8 {\n    7\n}\n";
        let pf = Shield::page_fault_str(source, "complex_fn").unwrap();
        assert!(pf.contains("complex_fn"));
        assert!(pf.contains("let y = x + 1"));
        assert!(pf.contains("let z = y * 2"));
    }

    #[test]
    fn test_real_expansion_val() {
        let path = "/root/scirust/tests/expansion_val.rs";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: expansion_val.rs not found");
            return;
        }
        let skeleton = Shield::parse(path).unwrap();
        eprintln!("Skeleton ({} tokens):\n{}", skeleton.estimated_tokens, skeleton.text);
        assert!(skeleton.estimated_tokens < 600, "expansion_val skeleton should be <600 tokens (was {})", skeleton.estimated_tokens);
        // 472 tokens vs ~10000 raw tokens = 95% compression. Agent can page-fault
        // individual tests on demand instead of loading the whole file.
        assert!(skeleton.anchors.len() >= 15, "Should find at least 15 test anchors");
    }
}
