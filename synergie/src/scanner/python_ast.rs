//! Extraction Python via regex — léger mais suffisant pour la détection
//! de fonctions/classes/méthodes publiques.

use crate::scanner::{ItemKind, PublicItem};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;

static RE_DEF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?P<indent>[ \t]*)def\s+(?P<name>\w+)\s*\((?P<args>[^)]*)\)").unwrap()
});
static RE_CLASS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^(?P<indent>[ \t]*)class\s+(?P<name>\w+)").unwrap());
static RE_DOC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*[rRbBuU]?["']{3}([\s\S]*?)["']{3}"#).unwrap());

pub fn parse_project(project_name: &str, files: &[PathBuf]) -> Result<Vec<PublicItem>> {
    let mut out = Vec::new();
    for f in files {
        if let Some(items) = parse_file(project_name, f) {
            out.extend(items);
        }
    }
    Ok(out)
}

fn parse_file(project: &str, path: &PathBuf) -> Option<Vec<PublicItem>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut items = Vec::new();
    let mut class_stack: Vec<(usize, String)> = Vec::new(); // (indent, name)

    // Mélange : on parcourt ligne à ligne pour gérer l'indentation/classes.
    let mut byte = 0usize;
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let bytes_in_line = line.len() + 1;

        // Sortir des classes finies.
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        while let Some((ci, _)) = class_stack.last() {
            if line.trim().is_empty() {
                break;
            }
            if indent > *ci {
                break;
            }
            class_stack.pop();
        }

        // class?
        if let Some(cap) = RE_CLASS.captures(line) {
            let name = cap.name("name").unwrap().as_str().to_string();
            if !name.starts_with('_') {
                items.push(PublicItem {
                    project: project.to_string(),
                    file: path.clone(),
                    line: line_no,
                    kind: ItemKind::PyClass,
                    name: name.clone(),
                    signature: format!("class {}", name),
                    body_tokens: extract_body_tokens(&text[byte..]),
                    doc: extract_docstring_after(&text, byte + bytes_in_line),
                });
            }
            class_stack.push((indent, name));
        }
        // def?
        else if let Some(cap) = RE_DEF.captures(line) {
            let name = cap.name("name").unwrap().as_str().to_string();
            // Filtre privé (sauf dunder qu'on garde).
            let is_dunder = name.starts_with("__") && name.ends_with("__");
            if name.starts_with('_') && !is_dunder {
                byte += bytes_in_line;
                continue;
            }
            let in_class = !class_stack.is_empty();
            let kind = if in_class {
                ItemKind::PyMethod
            } else {
                ItemKind::PyFunction
            };
            let args = cap.name("args").map(|m| m.as_str()).unwrap_or("");
            let signature = format!("def {}({})", name, args);
            items.push(PublicItem {
                project: project.to_string(),
                file: path.clone(),
                line: line_no,
                kind,
                name,
                signature,
                body_tokens: extract_body_tokens(&text[byte..]),
                doc: extract_docstring_after(&text, byte + bytes_in_line),
            });
        }

        byte += bytes_in_line;
    }

    Some(items)
}

fn extract_body_tokens(s: &str) -> Vec<String> {
    let snippet: String = s.chars().take(1500).collect();
    let mut out = Vec::new();
    let mut buf = String::new();
    for c in snippet.chars() {
        if c.is_alphanumeric() || c == '_' {
            buf.push(c);
        } else if !buf.is_empty() {
            if buf.len() >= 2 {
                out.push(buf.clone());
            }
            buf.clear();
        }
    }
    if buf.len() >= 2 {
        out.push(buf);
    }
    out
}

fn extract_docstring_after(text: &str, start: usize) -> Option<String> {
    let snippet = text.get(start..)?;
    let head: String = snippet.chars().take(800).collect();
    RE_DOC
        .captures(&head)
        .map(|c| c.get(1).unwrap().as_str().trim().to_string())
}

/// Extrait les imports d'un projet (utile pour deps_graph fallback).
pub fn imports_of(project: &PathBuf) -> Vec<String> {
    let mut out = Vec::new();
    let re_import =
        Regex::new(r"(?m)^\s*(?:from\s+([\w\.]+)\s+import|import\s+([\w\.]+))").unwrap();
    if let Ok(text) = std::fs::read_to_string(project) {
        for cap in re_import.captures_iter(&text) {
            let name = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str().to_string());
            if let Some(n) = name {
                out.push(n.split('.').next().unwrap_or(&n).to_string());
            }
        }
    }
    out
}
