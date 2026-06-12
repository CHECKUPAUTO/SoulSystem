//! Semantic diff — AST-aware comparison of two code fragments.
//!
//! Detects renames, extractions, inlines, type changes, and structural shifts
//! without relying on string diffing alone.

use regex::Regex;
use std::collections::HashMap;

/// Kinds of semantic change detected between two code versions.
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticChange {
    /// A variable was renamed: `old → new`
    RenameVar { old: String, new: String },
    /// A code block was extracted into a named function
    ExtractFn { name: String, body: String },
    /// A variable was inlined (used only once → value inserted)
    InlineVar { name: String, value: String },
    /// A type annotation was added/changed
    TypeHint {
        var: String,
        old_ty: Option<String>,
        new_ty: String,
    },
    /// Lines of code added in a block
    CodeAdded { block: String, lines: usize },
    /// Lines of code removed from a block
    CodeRemoved { block: String, lines: usize },
    /// A function signature changed
    SignatureChange {
        fn_name: String,
        old_sig: String,
        new_sig: String,
    },
    /// Generic change
    Other { desc: String },
}

impl SemanticChange {
    pub fn label(&self) -> &str {
        match self {
            SemanticChange::RenameVar { .. } => "rename_var",
            SemanticChange::ExtractFn { .. } => "extract_fn",
            SemanticChange::InlineVar { .. } => "inline_var",
            SemanticChange::TypeHint { .. } => "type_hint",
            SemanticChange::CodeAdded { .. } => "code_added",
            SemanticChange::CodeRemoved { .. } => "code_removed",
            SemanticChange::SignatureChange { .. } => "signature_change",
            SemanticChange::Other { .. } => "other",
        }
    }

    pub fn severity(&self) -> u8 {
        match self {
            SemanticChange::RenameVar { .. } => 1,
            SemanticChange::ExtractFn { .. } => 3,
            SemanticChange::InlineVar { .. } => 2,
            SemanticChange::TypeHint { .. } => 1,
            SemanticChange::SignatureChange { .. } => 4,
            SemanticChange::CodeAdded { .. } | SemanticChange::CodeRemoved { .. } => 2,
            SemanticChange::Other { .. } => 0,
        }
    }
}

/// Run full semantic diff between two code strings.
pub fn diff_semantic(before: &str, after: &str) -> Vec<SemanticChange> {
    let mut changes = Vec::new();

    // 1. Variable rename detection
    let before_vars = extract_vars(before);
    let after_vars = extract_vars(after);

    // Detect renames: same value, different name
    let mut matched_before: HashMap<String, bool> = HashMap::new();
    for (a_name, a_val) in &after_vars {
        for (b_name, b_val) in &before_vars {
            if matched_before.contains_key(b_name) {
                continue;
            }
            if a_val == b_val && a_name != b_name {
                changes.push(SemanticChange::RenameVar {
                    old: b_name.clone(),
                    new: a_name.clone(),
                });
                matched_before.insert(b_name.clone(), true);
                break;
            }
        }
    }

    // 2. Function extraction detection
    let before_fns = extract_funcs(before);
    let after_fns = extract_funcs(after);

    // Detect new functions whose body appears in the before code
    for (fn_name, fn_body) in &after_fns {
        if !before_fns.contains_key(fn_name) {
            if fn_body.len() > 20 && before.contains(fn_body) {
                changes.push(SemanticChange::ExtractFn {
                    name: fn_name.clone(),
                    body: fn_body.chars().take(80).collect(),
                });
            }
            // Check if the body originated from an inline block in before
            for (old_name, old_body) in &before_fns {
                if old_name != fn_name
                    && !fn_body.contains(old_name.as_str())
                    && old_body.len() > fn_body.len() / 2
                {
                    // Potential rename+extract
                    if fn_body.lines().count() >= old_body.lines().count() / 2 {
                        changes.push(SemanticChange::ExtractFn {
                            name: fn_name.clone(),
                            body: fn_body.chars().take(80).collect(),
                        });
                    }
                }
            }
        }
    }

    // 3. Variable inlining: var appears in before but not in after
    //    and the value appears inline in after
    for (b_name, b_val) in &before_vars {
        if !after_vars.contains_key(b_name) && !after.contains(b_val) {
            // Check if the var name disappeared (inlined)
            if after.contains(b_val.as_str()) && !after.contains(b_name.as_str()) {
                changes.push(SemanticChange::InlineVar {
                    name: b_name.clone(),
                    value: b_val.clone(),
                });
            }
        }
    }

    // 4. Function signature changes
    for (fn_name, new_sig) in extract_signatures(after) {
        if let Some(old_sig) = extract_signatures(before).get(&fn_name) {
            if old_sig != &new_sig {
                changes.push(SemanticChange::SignatureChange {
                    fn_name: fn_name.clone(),
                    old_sig: old_sig.clone(),
                    new_sig: new_sig.clone(),
                });
            }
        }
    }

    // 5. Code size deltas for the overall diff
    let before_lines = before.lines().count();
    let after_lines = after.lines().count();
    if after_lines > before_lines + 2 {
        // Try to find the block that grew
        if let Some(block) = find_grown_block(before, after) {
            changes.push(SemanticChange::CodeAdded {
                block,
                lines: after_lines - before_lines,
            });
        }
    } else if before_lines > after_lines + 2 {
        if let Some(block) = find_grown_block(after, before) {
            changes.push(SemanticChange::CodeRemoved {
                block,
                lines: before_lines - after_lines,
            });
        }
    }

    // 6. Type hint detection (Rust-specific: look for `: Type` additions)
    let types_before: Vec<(String, String)> = extract_type_hints(before);
    let types_after: Vec<(String, String)> = extract_type_hints(after);
    for (var, new_ty) in &types_after {
        if let Some((_, old_ty)) = types_before.iter().find(|(v, _)| v == var) {
            if old_ty != new_ty {
                changes.push(SemanticChange::TypeHint {
                    var: var.clone(),
                    old_ty: Some(old_ty.clone()),
                    new_ty: new_ty.clone(),
                });
            }
        } else {
            // New type hint
            changes.push(SemanticChange::TypeHint {
                var: var.clone(),
                old_ty: None,
                new_ty: new_ty.clone(),
            });
        }
    }

    // 7. Fallback: if nothing specific detected, report generic delta
    if changes.is_empty() {
        let before_len = before.len() as isize;
        let after_len = after.len() as isize;
        let added = after_len - before_len;
        let line_delta = after_lines as isize - before_lines as isize;
        changes.push(SemanticChange::Other {
            desc: if added > 0 {
                format!("+{added} chars / +{line_delta} lines")
            } else if added < 0 {
                format!("{added} chars / {line_delta} lines")
            } else {
                "unchanged size, non-semantic changes".to_string()
            },
        });
    }

    changes
}

// ─── Internal helpers ───────────────────────────────────────────────

fn extract_vars(code: &str) -> HashMap<String, String> {
    let re = Regex::new(r"(?:let\s+)?(\w+)\s*=\s*([^;\n{]+)").unwrap();
    re.captures_iter(code)
        .map(|c| {
            let name = c.get(1).unwrap().as_str().to_string();
            let val = c.get(2).unwrap().as_str().trim().to_string();
            (name, val)
        })
        .collect()
}

fn extract_funcs(code: &str) -> HashMap<String, String> {
    let mut funcs = HashMap::new();
    let mut depth = 0i32;
    let mut current_name = String::new();
    let mut current_body = String::new();
    let mut in_fn = false;
    let re_fn = Regex::new(r"^\s*(pub\s+)?(async\s+)?fn\s+(\w+)").unwrap();

    for line in code.lines() {
        if let Some(caps) = re_fn.captures(line) {
            if in_fn && !current_name.is_empty() {
                funcs.insert(current_name.clone(), current_body.clone());
            }
            current_name = caps.get(3).unwrap().as_str().to_string();
            current_body = line.to_string() + "\n";
            depth = line.chars().filter(|&c| c == '{').count() as i32
                - line.chars().filter(|&c| c == '}').count() as i32;
            in_fn = true;
        } else if in_fn {
            current_body.push_str(line);
            current_body.push('\n');
            depth += line.chars().filter(|&c| c == '{').count() as i32;
            depth -= line.chars().filter(|&c| c == '}').count() as i32;
            if depth <= 0 {
                funcs.insert(current_name.clone(), current_body.clone());
                in_fn = false;
                current_name.clear();
                current_body.clear();
            }
        }
    }
    if in_fn && !current_name.is_empty() {
        funcs.insert(current_name, current_body);
    }
    funcs
}

fn extract_signatures(code: &str) -> HashMap<String, String> {
    let re = Regex::new(r"(pub\s+)?(async\s+)?(unsafe\s+)?fn\s+(\w+)\s*\(([^)]*)\)\s*(->\s*\S+)?")
        .unwrap();
    re.captures_iter(code)
        .map(|c| {
            let name = c.get(4).unwrap().as_str().to_string();
            let params = c.get(5).map(|m| m.as_str()).unwrap_or("");
            let ret = c.get(6).map(|m| m.as_str()).unwrap_or("");
            (name, format!("fn({params}){ret}"))
        })
        .collect()
}

fn extract_type_hints(code: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"(?:let\s+)?(\w+)\s*:\s*(\w+(?:<[^>]+>)?)").unwrap();
    re.captures_iter(code)
        .map(|c| {
            let var = c.get(1).unwrap().as_str().to_string();
            let ty = c.get(2).unwrap().as_str().to_string();
            (var, ty)
        })
        .collect()
}

fn find_grown_block(shorter: &str, longer: &str) -> Option<String> {
    let short_lines: Vec<&str> = shorter.lines().collect();
    let long_lines: Vec<&str> = longer.lines().collect();
    if long_lines.len() <= short_lines.len() {
        return None;
    }

    // Find a contiguous block that exists in longer but not shorter
    let mut max_block = String::new();
    let mut best_len = 0usize;

    for start in 0..long_lines.len() {
        for end in start + 2..=long_lines.len().min(start + 15) {
            let block = long_lines[start..end].join("\n");
            if block.len() > best_len && !shorter.contains(&block) {
                max_block = block.chars().take(60).collect();
                best_len = block.len();
            }
        }
    }

    if best_len > 20 {
        Some(max_block)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename_detection() {
        let before = "let x = 42;";
        let after = "let y = 42;";
        let changes = diff_semantic(before, after);
        assert!(changes.iter().any(
            |c| matches!(c, SemanticChange::RenameVar { old, new } if old == "x" && new == "y")
        ));
    }

    #[test]
    fn test_empty_diff() {
        let code = "fn main() { let x = 1; }";
        let changes = diff_semantic(code, code);
        // Should have an "Other" change since nothing semantic changed
        assert!(!changes.is_empty());
    }

    #[test]
    fn test_fn_signature_change() {
        let before = "fn foo(x: i32) -> i32 { x }";
        let after = "fn foo(x: f64) -> f64 { x as f64 }";
        let changes = diff_semantic(before, after);
        assert!(changes
            .iter()
            .any(|c| matches!(c, SemanticChange::SignatureChange { .. })));
    }

    #[test]
    fn test_type_hint_added() {
        let before = "let x = 1;";
        let after = "let x: i32 = 1;";
        let changes = diff_semantic(before, after);
        assert!(changes
            .iter()
            .any(|c| matches!(c, SemanticChange::TypeHint { .. })));
    }
}
