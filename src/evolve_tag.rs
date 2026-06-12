//! `// @evolve` annotation parser.
//!
//! Scans a Rust / Python / JS / Go source file for inline annotations that
//! mark functions as evolution targets. Used by the GitHub Actions workflow
//! (`.github/workflows/evolve-on-tag.yml`) and by `openevolve evolve-tag <file>`
//! to discover what to optimise without a separate config.
//!
//! Recognised forms:
//!
//! ```rust,ignore
//! // @evolve fitness=bench/sort.rs iterations=100 language=rust
//! fn sort(v: Vec<i64>) -> Vec<i64> { ... }
//! ```
//!
//! Python and Go style:
//!
//! ```python,ignore
//! # @evolve fitness=./eval.py iterations=50 language=python
//! def solve(x): ...
//! ```
//!
//! Keys are optional except `fitness`. Unknown keys are ignored. The annotation
//! must be on the line immediately above the function declaration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolveTag {
    pub fitness: PathBuf,
    pub iterations: Option<u32>,
    pub language: Option<String>,
    pub function_name: Option<String>,
    pub source_file: PathBuf,
    pub line: usize,
}

pub fn parse_file(path: &Path) -> Result<Vec<EvolveTag>> {
    let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(parse_source(&src, path))
}

pub fn parse_source(source: &str, path: &Path) -> Vec<EvolveTag> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let payload = if let Some(p) = trimmed.strip_prefix("// @evolve") {
            p
        } else if let Some(p) = trimmed.strip_prefix("# @evolve") {
            p
        } else {
            continue;
        };
        let kv = parse_kv(payload);
        let Some(fitness) = kv.get("fitness").cloned() else {
            // skip annotations without a fitness target — they're useless
            continue;
        };
        let iterations = kv.get("iterations").and_then(|s| s.parse().ok());
        let language = kv.get("language").cloned();
        // Look ahead for the function declaration on the next non-empty line.
        let function_name = lines
            .iter()
            .skip(i + 1)
            .find(|l| !l.trim().is_empty())
            .and_then(|l| extract_fn_name(l));
        out.push(EvolveTag {
            fitness: PathBuf::from(fitness),
            iterations,
            language,
            function_name,
            source_file: path.to_path_buf(),
            line: i + 1,
        });
    }
    out
}

fn parse_kv(s: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for tok in s.split_whitespace() {
        if let Some((k, v)) = tok.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

fn extract_fn_name(line: &str) -> Option<String> {
    let t = line.trim_start();
    // Rust: pub fn name( | fn name(
    if let Some(rest) = t.strip_prefix("pub fn ").or_else(|| t.strip_prefix("fn ")) {
        return rest.split(['(', '<', ' ']).next().map(|s| s.to_string());
    }
    // Python: def name(
    if let Some(rest) = t.strip_prefix("def ") {
        return rest.split(['(', ' ']).next().map(|s| s.to_string());
    }
    // Go: func name(  |  func (recv) name(
    if let Some(rest) = t.strip_prefix("func ") {
        let rest = rest.trim_start_matches('(');
        // Skip receiver section if present
        let after_recv = rest
            .find(')')
            .map(|i| rest[i + 1..].trim_start())
            .unwrap_or(rest);
        return after_recv
            .split(['(', '[', ' '])
            .next()
            .map(|s| s.to_string());
    }
    // JS / TS: function name( | const name = (
    if let Some(rest) = t.strip_prefix("function ") {
        return rest.split(['(', '<', ' ']).next().map(|s| s.to_string());
    }
    if let Some(rest) = t.strip_prefix("const ").or_else(|| t.strip_prefix("let ")) {
        return rest.split(['=', ':', ' ']).next().map(|s| s.to_string());
    }
    None
}

/// Walk a directory, return every `@evolve` tag discovered.
pub fn parse_directory(dir: &Path) -> Result<Vec<EvolveTag>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).max_depth(10) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(ext, "rs" | "py" | "js" | "ts" | "go") {
            continue;
        }
        if let Ok(tags) = parse_file(p) {
            out.extend(tags);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rust_annotation() {
        let src = "// @evolve fitness=bench/sort.rs iterations=100 language=rust
fn sort(v: Vec<i64>) -> Vec<i64> { v }
";
        let tags = parse_source(src, Path::new("foo.rs"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].fitness, PathBuf::from("bench/sort.rs"));
        assert_eq!(tags[0].iterations, Some(100));
        assert_eq!(tags[0].language.as_deref(), Some("rust"));
        assert_eq!(tags[0].function_name.as_deref(), Some("sort"));
    }

    #[test]
    fn parses_python_annotation() {
        let src = "# @evolve fitness=./eval.py iterations=50 language=python
def solve(x):
    return x
";
        let tags = parse_source(src, Path::new("solve.py"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].function_name.as_deref(), Some("solve"));
        assert_eq!(tags[0].iterations, Some(50));
    }

    #[test]
    fn parses_go_with_receiver() {
        let src = "// @evolve fitness=eval.go
func (r *Repo) Sort(v []int) []int { return v }
";
        let tags = parse_source(src, Path::new("repo.go"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].function_name.as_deref(), Some("Sort"));
    }

    #[test]
    fn ignores_annotation_without_fitness() {
        let src = "// @evolve iterations=10
fn x() {}
";
        assert!(parse_source(src, Path::new("x.rs")).is_empty());
    }

    #[test]
    fn finds_multiple_tags() {
        let src = "// @evolve fitness=a.rs
fn one() {}

// @evolve fitness=b.rs
fn two() {}
";
        let tags = parse_source(src, Path::new("x.rs"));
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].function_name.as_deref(), Some("one"));
        assert_eq!(tags[1].function_name.as_deref(), Some("two"));
    }
}
