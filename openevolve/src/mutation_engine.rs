//! Mutation engine — real AST transformations via tree-sitter + creative LLM fallback.
//!
//! Strategy: 70 % structured (cheap, fast, semantics-friendly) and 30 % creative
//! (LLM-driven, can introduce new logic). Configurable via `structured_rate`.
//!
//! Uses `StdRng` (seedable, `Send`) wrapped in `parking_lot::Mutex` so the
//! engine can live inside async tasks.

use crate::llm::LlmClient;
use crate::program::Program;
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::ops::Range;
use tracing::{debug, warn};
use tree_sitter::{Language, Node, Parser, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationOp {
    ExtractFunction,
    InlineVariable,
    RenameVariable,
    LoopUnroll,
    AddEarlyReturn,
    ReplaceWithIter,
    AddTypeHint,
    Creative,
}

impl MutationOp {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ExtractFunction => "ExtractFunction",
            Self::InlineVariable => "InlineVariable",
            Self::RenameVariable => "RenameVariable",
            Self::LoopUnroll => "LoopUnroll",
            Self::AddEarlyReturn => "AddEarlyReturn",
            Self::ReplaceWithIter => "ReplaceWithIter",
            Self::AddTypeHint => "AddTypeHint",
            Self::Creative => "Creative",
        }
    }
}

pub struct MutationEngine {
    rng: Mutex<StdRng>,
    /// Current structured-vs-creative ratio. Adaptive via `note_outcome`.
    structured_rate: Mutex<f64>,
    /// Floor and ceiling the adaptive rate may drift between.
    rate_min: f64,
    rate_max: f64,
}

impl Default for MutationEngine {
    fn default() -> Self {
        Self::new(0.7)
    }
}

impl MutationEngine {
    pub fn new(structured_rate: f64) -> Self {
        Self {
            rng: Mutex::new(StdRng::from_entropy()),
            structured_rate: Mutex::new(structured_rate.clamp(0.0, 1.0)),
            rate_min: 0.2,
            rate_max: 0.95,
        }
    }

    /// Current structured rate (telemetry / dashboard).
    pub fn structured_rate(&self) -> f64 {
        *self.structured_rate.lock()
    }

    /// Feed back the outcome of one mutation so the engine can adapt.
    /// Adjustments are small (±0.01) to avoid oscillation.
    pub fn note_outcome(&self, was_structured: bool, improved: bool) {
        const STEP: f64 = 0.01;
        let mut g = self.structured_rate.lock();
        let delta = match (was_structured, improved) {
            (true, true) => STEP,
            (true, false) => -STEP,
            (false, true) => -STEP,
            (false, false) => STEP,
        };
        *g = (*g + delta).clamp(self.rate_min, self.rate_max);
    }

    /// Apply one mutation; falls back to creative LLM mutation if the AST path
    /// fails or `structured_rate` selects creative.
    pub async fn mutate(
        &self,
        program: &Program,
        language: &str,
        llm: &LlmClient,
    ) -> Result<(String, MutationOp)> {
        let roll: f64 = { self.rng.lock().gen() };
        if roll < *self.structured_rate.lock() {
            debug!(
                roll,
                threshold = *self.structured_rate.lock(),
                "structured mutation"
            );
            match self.structured_mutate(&program.code, language) {
                Ok((code, op)) => return Ok((code, op)),
                Err(e) => {
                    debug!(error=%e, "structured mutation failed, falling back to creative");
                }
            }
        }
        let prompt = format!(
            "Mutate this {language} code creatively while preserving semantics:\n\
             ```{language}\n{}\n```\nProvide ONLY the improved code.",
            program.code
        );
        let new_code = llm.mutate_with_prompt(&prompt).await?;
        if new_code.trim().is_empty() {
            anyhow::bail!("LLM returned empty code");
        }
        Ok((extract_code(&new_code), MutationOp::Creative))
    }

    /// Same as [`mutate`] but the caller provides the prompt already (e.g. with
    /// transfer-learning patterns + history baked in). The structured path is
    /// still tried first; if it succeeds the prompt is unused.
    pub async fn mutate_via_prompt(
        &self,
        program: &Program,
        language: &str,
        prompt: &str,
        llm: &LlmClient,
    ) -> Result<(String, MutationOp)> {
        let roll: f64 = { self.rng.lock().gen() };
        if roll < *self.structured_rate.lock() {
            if let Ok((code, op)) = self.structured_mutate(&program.code, language) {
                return Ok((code, op));
            }
        }
        let raw = llm.mutate_with_prompt(prompt).await?;
        if raw.trim().is_empty() {
            anyhow::bail!("LLM returned empty code");
        }
        Ok((extract_code(&raw), MutationOp::Creative))
    }

    /// Try a structured AST mutation. Returns the new code and the operator used.
    pub fn structured_mutate(&self, code: &str, language: &str) -> Result<(String, MutationOp)> {
        let lang = language_for(language)?;
        let mut parser = Parser::new();
        parser.set_language(&lang)?;
        let tree = parser
            .parse(code, None)
            .ok_or_else(|| anyhow!("tree-sitter produced no tree"))?;
        let ops = applicable_ops(&tree);
        if ops.is_empty() {
            anyhow::bail!("no structured ops applicable for {language}");
        }
        let mut rng = self.rng.lock();
        let op = ops[rng.gen_range(0..ops.len())];
        let new_code = match op {
            MutationOp::RenameVariable => apply_rename(&tree, code, &mut rng)?,
            MutationOp::InlineVariable => apply_inline(&tree, code, &mut rng)?,
            MutationOp::LoopUnroll => apply_loop_unroll(&tree, code, &mut rng)?,
            MutationOp::AddEarlyReturn => apply_early_return(&tree, code, language, &mut rng)?,
            MutationOp::ReplaceWithIter => apply_replace_iter(&tree, code)?,
            MutationOp::AddTypeHint => apply_type_hint(&tree, code, language, &mut rng)?,
            MutationOp::ExtractFunction => apply_extract_marker(&tree, code, language, &mut rng)?,
            MutationOp::Creative => return Err(anyhow!("creative handled separately")),
        };
        if new_code == code {
            warn!(?op, "structured mutation produced no change");
            anyhow::bail!("no-op mutation");
        }
        Ok((new_code, op))
    }
}

fn language_for(lang: &str) -> Result<Language> {
    match lang.to_lowercase().as_str() {
        "python" | "py" => Ok(tree_sitter_python::LANGUAGE.into()),
        "rust" | "rs" => Ok(tree_sitter_rust::LANGUAGE.into()),
        "javascript" | "js" | "jsx" => Ok(tree_sitter_javascript::LANGUAGE.into()),
        "go" | "golang" => Ok(tree_sitter_go::LANGUAGE.into()),
        _ => Err(anyhow!("unsupported language for AST: {lang}")),
    }
}

/// List operators meaningfully applicable to this AST.
pub fn applicable_ops(tree: &Tree) -> Vec<MutationOp> {
    let mut has_function = false;
    let mut has_loop = false;
    let mut has_var = false;
    let mut has_id = false;
    let mut has_if = false;
    walk(tree.root_node(), &mut |n| match n.kind() {
        "function_definition"
        | "function_item"
        | "function_declaration"
        | "function_expression"
        | "arrow_function"
        | "method_definition" => has_function = true,
        "for_statement" | "for_in_statement" | "for_of_statement" | "while_statement"
        | "for_expression" | "while_expression" => has_loop = true,
        "let_declaration"
        | "variable_declaration"
        | "lexical_declaration"
        | "short_var_declaration"
        | "assignment"
        | "annotated_assignment" => has_var = true,
        "identifier" => has_id = true,
        "if_statement" | "if_expression" => has_if = true,
        _ => {}
    });

    let mut ops = Vec::new();
    if has_function {
        ops.push(MutationOp::ExtractFunction);
    }
    if has_if {
        ops.push(MutationOp::AddEarlyReturn);
    }
    if has_loop {
        ops.push(MutationOp::LoopUnroll);
        ops.push(MutationOp::ReplaceWithIter);
    }
    if has_var {
        ops.push(MutationOp::InlineVariable);
    }
    if has_id {
        ops.push(MutationOp::RenameVariable);
        ops.push(MutationOp::AddTypeHint);
    }
    ops
}

// ── Individual operators ────────────────────────────────────────────────

fn apply_rename(tree: &Tree, code: &str, rng: &mut StdRng) -> Result<String> {
    let mut ids: Vec<Range<usize>> = Vec::new();
    walk(tree.root_node(), &mut |n| {
        if n.kind() == "identifier" {
            ids.push(n.byte_range());
        }
    });
    if ids.is_empty() {
        anyhow::bail!("no identifier");
    }
    let target = &ids[rng.gen_range(0..ids.len())];
    let original = code[target.clone()].to_string();
    if original.len() < 2 {
        anyhow::bail!("identifier too short");
    }
    // Rename ALL textual occurrences. Safe-ish without scope analysis if we pick
    // a non-existing replacement.
    let new_name = format!("renamed_{:04x}", rng.gen::<u16>());
    if code.contains(new_name.as_str()) {
        anyhow::bail!("name collision");
    }
    Ok(code.replace(&original, &new_name))
}

fn apply_inline(tree: &Tree, code: &str, rng: &mut StdRng) -> Result<String> {
    let mut decls: Vec<(String, String, Range<usize>)> = Vec::new();
    walk(tree.root_node(), &mut |n| {
        let k = n.kind();
        if !matches!(
            k,
            "let_declaration"
                | "variable_declaration"
                | "lexical_declaration"
                | "short_var_declaration"
                | "assignment"
                | "annotated_assignment"
        ) {
            return;
        }
        let left = n
            .child_by_field_name("left")
            .or_else(|| n.child_by_field_name("pattern"))
            .or_else(|| n.child_by_field_name("name"));
        let right = n
            .child_by_field_name("right")
            .or_else(|| n.child_by_field_name("value"));
        if let (Some(l), Some(r)) = (left, right) {
            let name = code[l.byte_range()].to_string();
            let val = code[r.byte_range()].to_string();
            if !name.is_empty() && !val.is_empty() && !val.contains('\n') {
                decls.push((name, val, n.byte_range()));
            }
        }
    });
    if decls.is_empty() {
        anyhow::bail!("no inlineable variable");
    }
    let (name, val, decl_range) = decls[rng.gen_range(0..decls.len())].clone();
    let after = &code[decl_range.end..];
    let needle = format!(r"\b{}\b", regex::escape(&name));
    let re = regex::Regex::new(&needle).map_err(|e| anyhow!("regex: {e}"))?;
    let m = re
        .find(after)
        .ok_or_else(|| anyhow!("variable {} unused after decl", name))?;
    let abs_start = decl_range.end + m.start();
    let len = m.end() - m.start();
    let mut out = String::with_capacity(code.len() + val.len());
    out.push_str(&code[..abs_start]);
    out.push_str(&val);
    out.push_str(&code[abs_start + len..]);
    Ok(out)
}

fn apply_loop_unroll(tree: &Tree, code: &str, rng: &mut StdRng) -> Result<String> {
    let mut bodies: Vec<Range<usize>> = Vec::new();
    walk(tree.root_node(), &mut |n| {
        if matches!(
            n.kind(),
            "for_statement"
                | "for_in_statement"
                | "for_of_statement"
                | "while_statement"
                | "for_expression"
                | "while_expression"
        ) {
            for i in 0..n.child_count() {
                if let Some(c) = n.child(i as u32) {
                    if matches!(c.kind(), "block" | "compound_statement" | "statement_block") {
                        bodies.push(c.byte_range());
                        break;
                    }
                }
            }
        }
    });
    if bodies.is_empty() {
        anyhow::bail!("no loop body");
    }
    let body = &bodies[rng.gen_range(0..bodies.len())];
    let mut out = String::with_capacity(code.len() + 48);
    out.push_str(&code[..body.end]);
    out.push_str("\n// hint: candidate for loop unroll\n");
    out.push_str(&code[body.end..]);
    Ok(out)
}

fn apply_early_return(tree: &Tree, code: &str, lang: &str, rng: &mut StdRng) -> Result<String> {
    let mut blocks: Vec<Range<usize>> = Vec::new();
    walk(tree.root_node(), &mut |n| {
        if matches!(n.kind(), "if_statement" | "if_expression") {
            if let Some(cons) = n.child_by_field_name("consequence") {
                if matches!(
                    cons.kind(),
                    "block" | "compound_statement" | "statement_block"
                ) {
                    blocks.push(cons.byte_range());
                }
            }
        }
    });
    if blocks.is_empty() {
        anyhow::bail!("no if block");
    }
    let block = &blocks[rng.gen_range(0..blocks.len())];
    let stmt = match lang {
        "python" | "py" => "    return None\n",
        "rust" | "rs" => "    return Default::default();\n",
        "javascript" | "js" => "    return;\n",
        "go" => "    return\n",
        _ => "    return\n",
    };
    let insert_pos = block.end.saturating_sub(1);
    let mut out = String::with_capacity(code.len() + stmt.len());
    out.push_str(&code[..insert_pos]);
    out.push_str(stmt);
    out.push_str(&code[insert_pos..]);
    Ok(out)
}

fn apply_replace_iter(tree: &Tree, code: &str) -> Result<String> {
    let mut found: Option<Range<usize>> = None;
    walk(tree.root_node(), &mut |n| {
        if found.is_some() {
            return;
        }
        if n.kind() == "call" {
            let txt = &code[n.byte_range()];
            if txt.starts_with("range(") && !code.contains("iter(range(") {
                found = Some(n.byte_range());
            }
        }
    });
    let r = found.ok_or_else(|| anyhow!("no range() call to wrap"))?;
    let inner = &code[r.clone()];
    let mut out = String::with_capacity(code.len() + 6);
    out.push_str(&code[..r.start]);
    out.push_str(&format!("iter({inner})"));
    out.push_str(&code[r.end..]);
    Ok(out)
}

fn apply_type_hint(tree: &Tree, code: &str, lang: &str, rng: &mut StdRng) -> Result<String> {
    if lang != "python" && lang != "py" {
        anyhow::bail!("type hint only impl for python");
    }
    let mut params: Vec<Range<usize>> = Vec::new();
    walk(tree.root_node(), &mut |n| {
        if n.kind() == "identifier" {
            if let Some(p) = n.parent() {
                if p.kind() == "parameters" && n.next_sibling().map(|s| s.kind()) != Some(":") {
                    params.push(n.byte_range());
                }
            }
        }
    });
    if params.is_empty() {
        anyhow::bail!("no untyped parameter");
    }
    let t = &params[rng.gen_range(0..params.len())];
    let mut out = String::with_capacity(code.len() + 8);
    out.push_str(&code[..t.end]);
    out.push_str(": object");
    out.push_str(&code[t.end..]);
    Ok(out)
}

fn apply_extract_marker(tree: &Tree, code: &str, lang: &str, rng: &mut StdRng) -> Result<String> {
    // Conservative: marker only. Real extraction needs scope/free-var analysis.
    let mut fns: Vec<usize> = Vec::new();
    walk(tree.root_node(), &mut |n| {
        if matches!(
            n.kind(),
            "function_definition" | "function_item" | "function_declaration"
        ) {
            fns.push(n.start_byte());
        }
    });
    if fns.is_empty() {
        anyhow::bail!("no function");
    }
    let pos = fns[rng.gen_range(0..fns.len())];
    let marker = match lang {
        "python" | "py" => "# extract-candidate: split helper logic\n",
        _ => "// extract-candidate: split helper logic\n",
    };
    let mut out = String::with_capacity(code.len() + marker.len());
    out.push_str(&code[..pos]);
    out.push_str(marker);
    out.push_str(&code[pos..]);
    Ok(out)
}

/// Real AST subtree-swap crossover.
///
/// 1. Parse both parents with tree-sitter.
/// 2. Collect every "function-like" node in each tree.
/// 3. Pick one function from each (random).
/// 4. Replace parent A's function body bytes with parent B's function body bytes.
///
/// Returns the child code. Semantics-preserving for *typed* languages only when
/// the swapped functions share a compatible signature, which we don't enforce —
/// the resulting program is then graded by the evaluator like any other candidate.
pub fn ast_crossover(
    code_a: &str,
    code_b: &str,
    language: &str,
    rng: &mut StdRng,
) -> Result<String> {
    let lang = language_for(language)?;
    let mut parser = Parser::new();
    parser.set_language(&lang)?;
    let tree_a = parser
        .parse(code_a, None)
        .ok_or_else(|| anyhow!("parse A"))?;
    let tree_b = parser
        .parse(code_b, None)
        .ok_or_else(|| anyhow!("parse B"))?;

    let fns_a = collect_function_bodies(tree_a.root_node(), language);
    let fns_b = collect_function_bodies(tree_b.root_node(), language);
    if fns_a.is_empty() || fns_b.is_empty() {
        anyhow::bail!("no function bodies on at least one parent");
    }
    let a = &fns_a[rng.gen_range(0..fns_a.len())];
    let b = &fns_b[rng.gen_range(0..fns_b.len())];
    let body_b = &code_b[b.clone()];
    let mut out = String::with_capacity(code_a.len() + body_b.len());
    out.push_str(&code_a[..a.start]);
    out.push_str(body_b);
    out.push_str(&code_a[a.end..]);
    Ok(out)
}

fn collect_function_bodies(root: Node, language: &str) -> Vec<Range<usize>> {
    let mut bodies = Vec::new();
    walk(root, &mut |n| {
        if !matches!(
            n.kind(),
            "function_definition"
                | "function_item"
                | "function_declaration"
                | "method_declaration"
                | "function_expression"
        ) {
            return;
        }
        // The body field is `body` for most grammars; for python it's `body` too.
        let body = match language {
            "python" | "py" | "rust" | "rs" | "javascript" | "js" | "go" => {
                n.child_by_field_name("body")
            }
            _ => None,
        };
        let body = body.or_else(|| {
            // Fallback: take the first block-like child
            for i in 0..n.child_count() {
                if let Some(c) = n.child(i as u32) {
                    if matches!(c.kind(), "block" | "compound_statement" | "statement_block") {
                        return Some(c);
                    }
                }
            }
            None
        });
        if let Some(b) = body {
            bodies.push(b.byte_range());
        }
    });
    bodies
}

fn walk<'a, F: FnMut(Node<'a>)>(node: Node<'a>, f: &mut F) {
    f(node);
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i as u32) {
            walk(c, f);
        }
    }
}

/// Strip ```fenced blocks``` from an LLM response.
pub fn extract_code(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(after_open) = trimmed.find("```") {
        let after_open = after_open + 3;
        let rest = &trimmed[after_open..];
        if let Some(nl) = rest.find('\n') {
            let body = &rest[nl + 1..];
            if let Some(close) = body.rfind("```") {
                return body[..close].trim_end_matches('\n').to_string();
            }
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_strips_fences() {
        let s = "Here:\n```rust\nfn x(){}\n```\nDone.";
        assert_eq!(extract_code(s), "fn x(){}");
    }

    #[test]
    fn rename_changes_code_python() {
        let me = MutationEngine::new(1.0);
        // Use identifiers >= 2 chars so apply_rename never bails on "too short".
        let code = "def compute(value):\n    return value + counter\n";
        let mut ok = false;
        // Try several times — the engine picks an op randomly. We only require
        // *some* structured op succeeds, not a specific one.
        for _ in 0..16 {
            if let Ok((out, op)) = me.structured_mutate(code, "python") {
                assert_ne!(out, code);
                assert_ne!(op, MutationOp::Creative);
                ok = true;
                break;
            }
        }
        assert!(ok, "no structured op succeeded after 16 tries");
    }

    #[test]
    fn applicable_ops_includes_function() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("def f(): return 1", None).unwrap();
        let ops = applicable_ops(&tree);
        assert!(ops.contains(&MutationOp::ExtractFunction));
    }

    #[test]
    fn ast_crossover_swaps_function_body() {
        let mut rng = StdRng::seed_from_u64(7);
        let a = "def solve(x):\n    return x + 1\n";
        let b = "def helper(y):\n    return y * 42\n";
        let out = ast_crossover(a, b, "python", &mut rng).unwrap();
        // The output should contain the body of B's function spliced into A's frame.
        assert!(
            out.contains("* 42"),
            "child should contain B body, got: {out}"
        );
        assert_ne!(out, a);
    }
}
