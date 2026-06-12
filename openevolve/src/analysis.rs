//! Static code analysis using tree-sitter AST.
//!
//! Detects cyclomatic complexity, real nesting depth, unused imports/vars,
//! long functions, and dangerous patterns.
#![allow(
    clippy::manual_strip,
    clippy::regex_creation_in_loops,
    clippy::collapsible_match,
    clippy::collapsible_if
)]

use regex::Regex;
use std::sync::LazyLock;
use tree_sitter::{Language, Node, Parser, Tree};

#[derive(Debug, Clone, PartialEq)]
pub enum ComplexityClass {
    Constant,
    Log,
    Linear,
    Linearithmic,
    Quadratic,
    CubicPlus,
    Exponential,
    Factorial,
}

impl ComplexityClass {
    pub fn penalty(&self) -> f64 {
        match self {
            Self::Constant | Self::Log => 0.0,
            Self::Linear => 0.05,
            Self::Linearithmic => 0.1,
            Self::Quadratic => 0.3,
            Self::CubicPlus => 0.5,
            Self::Exponential => 0.7,
            Self::Factorial => 0.9,
        }
    }
    pub fn label(&self) -> &str {
        match self {
            Self::Constant => "O(1)",
            Self::Log => "O(log n)",
            Self::Linear => "O(n)",
            Self::Linearithmic => "O(n log n)",
            Self::Quadratic => "O(n²)",
            Self::CubicPlus => "O(n³+)",
            Self::Exponential => "O(2ⁿ)",
            Self::Factorial => "O(n!)",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AnalysisResult {
    pub cyclomatic_complexity: u32,
    pub max_nesting_depth: u32,
    pub loc: usize,
    pub function_count: usize,
    pub complexity_class: ComplexityClass,
    pub quality_score: f64,
    pub penalties: Vec<String>,
    pub unused_imports: Vec<String>,
    pub unused_variables: Vec<String>,
    pub long_functions: Vec<String>,
    pub security_warnings: Vec<String>,
}

// ── Language loading ────────────────────────────────────────

fn language_from_str(lang: &str) -> Option<Language> {
    match lang {
        "python" | "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "rust" | "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "javascript" | "js" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        _ => None,
    }
}

fn is_control_node(kind: &str, lang: &str) -> bool {
    let common = [
        "if_statement",
        "while_statement",
        "for_statement",
        "for_in_statement",
        "do_statement",
        "case",
        "catch_clause",
        "conditional_expression",
        "else_clause",
    ];
    if common.contains(&kind) {
        return true;
    }
    match lang {
        "python" | "py" => matches!(
            kind,
            "if_statement"
                | "while_statement"
                | "for_statement"
                | "except_clause"
                | "with_statement"
                | "boolean_operator"
                | "conditional_expression"
        ),
        "rust" | "rs" => matches!(
            kind,
            "if_expression"
                | "while_expression"
                | "for_expression"
                | "match_expression"
                | "match_arm"
                | "if_let_expression"
                | "conjunction_expression"
                | "disjunction_expression"
        ),
        "javascript" | "js" => matches!(
            kind,
            "if_statement"
                | "while_statement"
                | "for_statement"
                | "switch_statement"
                | "switch_case"
                | "catch_clause"
                | "ternary_expression"
                | "logical_expression"
        ),
        "go" => matches!(
            kind,
            "if_statement"
                | "for_statement"
                | "range_clause"
                | "switch_statement"
                | "case_clause"
                | "select_statement"
                | "communication_case"
        ),
        _ => false,
    }
}

fn is_function_node(kind: &str, lang: &str) -> bool {
    match lang {
        "python" | "py" => kind == "function_definition" || kind == "lambda",
        "rust" | "rs" => {
            kind == "function_item" || kind == "closure_expression" || kind == "method"
        }
        "javascript" | "js" => {
            kind == "function_declaration"
                || kind == "function_expression"
                || kind == "arrow_function"
                || kind == "method_definition"
        }
        "go" => {
            kind == "function_declaration" || kind == "func_literal" || kind == "method_declaration"
        }
        _ => false,
    }
}

fn is_import_node(kind: &str, lang: &str) -> bool {
    match lang {
        "python" | "py" => kind == "import_statement" || kind == "import_from_statement",
        "rust" | "rs" => kind == "use_declaration" || kind == "extern_crate",
        "javascript" | "js" => kind == "import_statement" || kind == "import_clause",
        "go" => kind == "import_declaration",
        _ => false,
    }
}

fn is_variable_declaration(kind: &str, lang: &str) -> bool {
    match lang {
        "python" | "py" => kind == "assignment" || kind == "annotated_assignment",
        "rust" | "rs" => {
            kind == "let_declaration"
                || kind == "const_item"
                || kind == "static_item"
                || kind == "variable_declaration"
        }
        "javascript" | "js" => {
            kind == "variable_declaration"
                || kind == "lexical_declaration"
                || kind == "assignment_expression"
        }
        "go" => {
            kind == "short_var_declaration"
                || kind == "var_declaration"
                || kind == "const_declaration"
        }
        _ => false,
    }
}

fn node_loc(node: Node, source: &str) -> usize {
    let start = node.start_position().row;
    let end = node.end_position().row;
    source
        .lines()
        .enumerate()
        .skip(start)
        .take(end.saturating_sub(start) + 1)
        .filter(|(_, l)| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("#") && !t.starts_with("//")
        })
        .count()
}

// ── Core analysis ───────────────────────────────────────────

pub fn analyze(code: &str, language: &str) -> AnalysisResult {
    let lang_str = language;
    let mut parser = Parser::new();
    if let Some(lang) = language_from_str(lang_str) {
        let _ = parser.set_language(&lang);
    }

    let tree: Option<Tree> = parser.parse(code, None);
    let root = tree.as_ref().map(|t| t.root_node());

    let loc = if let Some(r) = root {
        compute_loc_ast(r, code)
    } else {
        code.lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with('#') && !t.starts_with("//")
            })
            .count()
    };

    let mut cc: u32 = 1;
    let mut max_nesting: u32 = 0;
    let mut fn_count: usize = 0;
    let mut long_fns: Vec<String> = Vec::new();
    let mut imports: Vec<(String, Node)> = Vec::new();
    let mut declared_vars: Vec<(String, Node)> = Vec::new();
    let mut used_ids: Vec<String> = Vec::new();
    let mut security: Vec<String> = Vec::new();

    if let Some(r) = root {
        let mut stack: Vec<(&str, u32)> = Vec::new();
        let mut cursor = r.walk();
        'outer: loop {
            let node = cursor.node();
            let kind = node.kind();

            if is_control_node(kind, lang_str) {
                cc += 1;
                let depth = stack.len() as u32 + 1;
                max_nesting = max_nesting.max(depth);
                stack.push((kind, depth));
            }

            if is_function_node(kind, lang_str) {
                fn_count += 1;
                let floc = node_loc(node, code);
                if floc > 50 {
                    let name = function_name(node, code, lang_str);
                    long_fns.push(format!("{} ({} loc)", name, floc));
                }
            }

            if is_import_node(kind, lang_str) {
                if let Some(name) = extract_import_name(node, code, lang_str) {
                    imports.push((name, node));
                }
            }

            if is_variable_declaration(kind, lang_str) {
                for v in extract_variable_names(node, code, lang_str) {
                    declared_vars.push((v, node));
                }
            }

            if kind == "identifier" || kind == "type_identifier" || kind == "property_identifier" {
                let text = node_text(node, code);
                if !text.is_empty() {
                    used_ids.push(text);
                }
            }

            // Security heuristics
            match lang_str {
                "python" | "py" if kind == "call" => {
                    let txt = node_text(node, code);
                    if txt.contains("eval(") {
                        security.push("eval() detected".into());
                    }
                    if txt.contains("exec(") {
                        security.push("exec() detected".into());
                    }
                    if txt.contains("subprocess.call") || txt.contains("os.system") {
                        security.push("dangerous subprocess/os call".into());
                    }
                }
                "rust" | "rs" => {
                    if kind == "unsafe_block" {
                        security.push("unsafe block".into());
                    }
                    if kind == "call_expression" || kind == "macro_invocation" {
                        let txt = node_text(node, code);
                        if txt.contains("unwrap()") || txt.contains("unwrap_") {
                            security.push("unwrap() usage".into());
                        }
                        if txt.contains("panic!") {
                            security.push("panic! usage".into());
                        }
                    }
                }
                "javascript" | "js" if kind == "call_expression" => {
                    let txt = node_text(node, code);
                    if txt.contains("eval(") {
                        security.push("eval() detected".into());
                    }
                }
                "go" if kind == "call_expression" => {
                    let txt = node_text(node, code);
                    if txt.contains("unsafe.") {
                        security.push("unsafe package usage".into());
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                continue;
            }
            loop {
                let leaving = cursor.node().kind();
                if is_control_node(leaving, lang_str) {
                    stack.pop();
                }
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    break 'outer;
                }
            }
        }
    }

    // Unused imports
    let mut unused_imports = Vec::new();
    for (imp, _node) in &imports {
        let base = imp
            .split("::")
            .next()
            .unwrap_or(imp)
            .split('.')
            .next()
            .unwrap_or(imp);
        if !used_ids.iter().any(|id| id == base || id == imp) {
            unused_imports.push(imp.clone());
        }
    }

    // Unused variables (naive: declared but never referenced)
    let mut unused_vars = Vec::new();
    for (var, _node) in &declared_vars {
        let count = used_ids.iter().filter(|id| **id == *var).count();
        if count <= 1 {
            unused_vars.push(var.clone());
        }
    }

    let cpx = detect_complexity_ast(root, code, lang_str, &imports, &used_ids);

    let mut q = 1.0;
    let mut penalties = Vec::new();

    if cc > 20 {
        let p = ((cc - 20) as f64 * 0.01).min(0.3);
        q -= p;
        penalties.push(format!("cyclomatic={cc} p={p:.3}"));
    }
    if max_nesting > 5 {
        let p = ((max_nesting - 5) as f64 * 0.02).min(0.2);
        q -= p;
        penalties.push(format!("nesting={max_nesting} p={p:.3}"));
    }
    if fn_count > 0 {
        let avg = loc as f64 / fn_count as f64;
        if avg > 100.0 {
            let p = ((avg - 100.0) * 0.001).min(0.15);
            q -= p;
            penalties.push(format!("avg_loc={avg:.0} p={p:.3}"));
        }
    }
    let op = cpx.penalty();
    if op > 0.0 {
        q -= op;
        penalties.push(format!("{} p={}", cpx.label(), op));
    }
    if !unused_imports.is_empty() {
        let p = (unused_imports.len() as f64 * 0.01).min(0.1);
        q -= p;
        penalties.push(format!("unused_imports={} p={p:.3}", unused_imports.len()));
    }
    if !unused_vars.is_empty() {
        let p = (unused_vars.len() as f64 * 0.005).min(0.1);
        q -= p;
        penalties.push(format!("unused_vars={} p={p:.3}", unused_vars.len()));
    }
    if !long_fns.is_empty() {
        let p = (long_fns.len() as f64 * 0.01).min(0.1);
        q -= p;
        penalties.push(format!("long_fns={} p={p:.3}", long_fns.len()));
    }
    if !security.is_empty() {
        let p = (security.len() as f64 * 0.02).min(0.15);
        q -= p;
        penalties.push(format!("security={} p={p:.3}", security.len()));
    }

    q = q.clamp(0.0, 1.0);

    AnalysisResult {
        cyclomatic_complexity: cc,
        max_nesting_depth: max_nesting,
        loc,
        function_count: fn_count,
        complexity_class: cpx,
        quality_score: q,
        penalties,
        unused_imports,
        unused_variables: unused_vars,
        long_functions: long_fns,
        security_warnings: security,
    }
}

// ── AST helpers ─────────────────────────────────────────────

fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

fn function_name(node: Node, source: &str, lang: &str) -> String {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match lang {
                "python" | "py" if child.kind() == "identifier" => return node_text(child, source),
                "rust" | "rs" if child.kind() == "identifier" || child.kind() == "name" => {
                    return node_text(child, source);
                }
                "javascript" | "js"
                    if child.kind() == "identifier" || child.kind() == "property_identifier" =>
                {
                    return node_text(child, source);
                }
                "go" if child.kind() == "identifier" => return node_text(child, source),
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    "<anon>".into()
}

fn extract_import_name(node: Node, source: &str, lang: &str) -> Option<String> {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match lang {
                "python" | "py"
                    if (child.kind() == "dotted_name" || child.kind() == "identifier") =>
                {
                    return Some(node_text(child, source));
                }
                "rust" | "rs" if (child.kind() == "identifier" || child.kind() == "crate") => {
                    return Some(node_text(child, source));
                }
                "javascript" | "js"
                    if (child.kind() == "identifier" || child.kind() == "string") =>
                {
                    let t = node_text(child, source);
                    return Some(t.trim_matches('\'').trim_matches('"').to_string());
                }
                "go" if (child.kind() == "import_spec"
                    || child.kind() == "interpreted_string_literal") =>
                {
                    let t = node_text(child, source);
                    return Some(t.trim_matches('"').to_string());
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

fn extract_variable_names(node: Node, source: &str, _lang: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "identifier" || child.kind() == "pattern" {
                let t = node_text(child, source);
                if !t.is_empty() && !t.starts_with('_') {
                    out.push(t);
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    out
}

fn compute_loc_ast(root: Node, source: &str) -> usize {
    let mut count = 0usize;
    let mut cursor = root.walk();
    'outer: loop {
        let node = cursor.node();
        if node.child_count() == 0 {
            let row = node.start_position().row;
            let line = source.lines().nth(row).unwrap_or("");
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') && !t.starts_with("//") {
                count += 1;
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                break 'outer;
            }
        }
    }
    count
}

// ── Complexity detection via AST ──────────────────────────

fn detect_complexity_ast(
    root: Option<Node>,
    code: &str,
    lang: &str,
    _imports: &[(String, Node)],
    _used_ids: &[String],
) -> ComplexityClass {
    let Some(r) = root else {
        return fallback_complexity(code);
    };

    let mut has_for = false;
    let mut has_while = false;
    let mut has_recursion = false;
    let mut loop_depth = 0u32;
    let mut max_loop_depth = 0u32;
    let mut has_sort = false;
    let mut has_dc = false;

    let mut cursor = r.walk();
    let mut func_names: Vec<String> = Vec::new();

    'outer: loop {
        let node = cursor.node();
        let kind = node.kind();

        if is_function_node(kind, lang) {
            let name = function_name(node, code, lang);
            if !name.is_empty() && name != "<anon>" {
                func_names.push(name);
            }
        }

        let is_loop = matches!(
            kind,
            "for_statement"
                | "for_in_statement"
                | "while_statement"
                | "do_statement"
                | "for_expression"
                | "while_expression"
                | "range_clause"
        );
        if is_loop {
            has_for = true;
            loop_depth += 1;
            max_loop_depth = max_loop_depth.max(loop_depth);
        }
        if kind == "while_statement" || kind == "while_expression" {
            has_while = true;
        }

        let txt = node_text(node, code);
        if txt.contains("sort") || txt.contains("sorted") {
            has_sort = true;
        }
        if txt.contains("mid")
            || txt.contains("pivot")
            || txt.contains("partition")
            || txt.contains("merge")
            || txt.contains("half")
            || txt.contains("split")
        {
            has_dc = true;
        }

        if cursor.goto_first_child() {
            continue;
        }
        loop {
            let leaving = cursor.node().kind();
            if matches!(
                leaving,
                "for_statement"
                    | "for_in_statement"
                    | "while_statement"
                    | "do_statement"
                    | "for_expression"
                    | "while_expression"
                    | "range_clause"
            ) {
                loop_depth = loop_depth.saturating_sub(1);
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                break 'outer;
            }
        }
    }

    for f in &func_names {
        if code.matches(f.as_str()).count() > 1 {
            has_recursion = true;
            break;
        }
    }

    if has_recursion
        && (code.contains("permut") || code.contains("backtrack") || code.contains("combination"))
    {
        return ComplexityClass::Factorial;
    }
    if has_recursion && !has_dc {
        return ComplexityClass::Exponential;
    }
    if max_loop_depth >= 3 {
        return ComplexityClass::CubicPlus;
    }
    if max_loop_depth >= 2 {
        return ComplexityClass::Quadratic;
    }
    if (has_sort || has_dc) && (has_for || has_while) {
        return ComplexityClass::Linearithmic;
    }
    if has_for || has_while {
        return ComplexityClass::Linear;
    }
    if has_dc {
        return ComplexityClass::Log;
    }
    ComplexityClass::Constant
}

fn fallback_complexity(code: &str) -> ComplexityClass {
    static NESTED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ms)for[^:]*:.*\n.*for[^:]*:|\bwhile\b.*\{.*\bwhile\b.*\{|for\s*\([^)]*\)\s*\{[^}]*for\s*\([^)]*\)").unwrap()
    });
    static FOR_LP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*for\b").unwrap());
    static WHILE_LP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*while\b").unwrap());
    static DC_HINT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)(mid|pivot|partition|merge|half|split)").unwrap());
    static SORT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)\.(sort|sorted)\b").unwrap());

    let nested = NESTED.is_match(code);
    let has_for = FOR_LP.is_match(code);
    let has_while = WHILE_LP.is_match(code);
    let dc = DC_HINT.is_match(code);
    let sort = SORT_RE.is_match(code);

    if nested && (code.matches("for").count() >= 3 || code.matches("while").count() >= 3) {
        return ComplexityClass::CubicPlus;
    }
    if nested {
        return ComplexityClass::Quadratic;
    }
    if (sort || dc) && (has_for || has_while) {
        return ComplexityClass::Linearithmic;
    }
    if has_for || has_while {
        return ComplexityClass::Linear;
    }
    if dc {
        return ComplexityClass::Log;
    }
    ComplexityClass::Constant
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cyclomatic_python() {
        let code = r#"def f(x):
    if x > 0:
        return 1
    return 0
"#;
        let res = analyze(code, "python");
        assert!(
            res.cyclomatic_complexity >= 2,
            "cc={}",
            res.cyclomatic_complexity
        );
    }

    #[test]
    fn test_nesting_python() {
        let code = r#"def g():
    for i in range(10):
        if i > 5:
            while True:
                pass
"#;
        let res = analyze(code, "python");
        assert!(
            res.max_nesting_depth >= 3,
            "nesting={}",
            res.max_nesting_depth
        );
    }

    #[test]
    fn test_long_function() {
        let mut code = String::from("def long_fn():\n");
        for i in 0..60 {
            code.push_str(&format!("    x{} = {}\n", i, i));
        }
        let res = analyze(&code, "python");
        assert!(!res.long_functions.is_empty(), "should detect long fn");
    }

    #[test]
    fn test_complexity_quadratic() {
        let code = r#"for i in range(n):
    for j in range(m):
        pass
"#;
        let res = analyze(code, "python");
        assert_eq!(res.complexity_class, ComplexityClass::Quadratic);
    }

    #[test]
    fn test_complexity_linear() {
        let code = r#"for i in range(n):
    pass
"#;
        let res = analyze(code, "python");
        assert_eq!(res.complexity_class, ComplexityClass::Linear);
    }

    #[test]
    fn test_constant() {
        let res = analyze("x = 1 + 2", "python");
        assert_eq!(res.complexity_class, ComplexityClass::Constant);
        assert!(res.quality_score > 0.95);
    }

    #[test]
    fn test_rust_analysis() {
        let code = r#"fn main() {
    let x = 5;
    if x > 0 {
        println!("yes");
    }
}
"#;
        let res = analyze(code, "rust");
        assert!(res.cyclomatic_complexity >= 2);
    }

    #[test]
    fn test_js_analysis() {
        let code = r#"function foo() {
    for (let i=0; i<10; i++) {
        if (i > 5) console.log(i);
    }
}
"#;
        let res = analyze(code, "javascript");
        assert!(res.cyclomatic_complexity >= 3);
        assert!(res.function_count >= 1);
    }

    #[test]
    fn test_go_analysis() {
        let code = r#"package main
func main() {
    for i := 0; i < 10; i++ {
        if i > 5 {
            fmt.Println(i)
        }
    }
}
"#;
        let res = analyze(code, "go");
        assert!(res.cyclomatic_complexity >= 3);
    }
}

// ── Call-graph analysis (Phase 2) ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

impl CallGraph {
    /// Extract a simple call graph from source code using regex.
    /// For production, replace with tree-sitter AST traversal.
    pub fn from_code(code: &str, language: &str) -> Self {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut current_fn = None;

        for line in code.lines() {
            let trimmed = line.trim();
            match language {
                "python" | "py" => {
                    if trimmed.starts_with("def ") {
                        let name = trimmed[4..].split('(').next().unwrap_or("").trim();
                        current_fn = Some(name.to_string());
                        nodes.push(name.to_string());
                    } else if let Some(ref caller) = current_fn {
                        for cap in regex::Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(")
                            .unwrap()
                            .captures_iter(trimmed)
                        {
                            if let Some(m) = cap.get(1) {
                                let callee = m.as_str();
                                if callee != "def" && callee != "if" && callee != "while" {
                                    edges.push((caller.clone(), callee.to_string()));
                                }
                            }
                        }
                    }
                }
                "rust" | "rs" => {
                    if trimmed.starts_with("fn ") {
                        let name = trimmed[3..].split('(').next().unwrap_or("").trim();
                        current_fn = Some(name.to_string());
                        nodes.push(name.to_string());
                    } else if let Some(ref caller) = current_fn {
                        for cap in regex::Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(")
                            .unwrap()
                            .captures_iter(trimmed)
                        {
                            if let Some(m) = cap.get(1) {
                                let callee = m.as_str();
                                if callee != "fn" {
                                    edges.push((caller.clone(), callee.to_string()));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        CallGraph { nodes, edges }
    }

    /// Detect orphaned functions (no incoming calls except themselves).
    pub fn orphans(&self) -> Vec<&String> {
        let called: std::collections::HashSet<_> = self.edges.iter().map(|(_, c)| c).collect();
        self.nodes.iter().filter(|n| !called.contains(*n)).collect()
    }

    /// Detect cycles via DFS.
    pub fn cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut path = Vec::new();

        for node in &self.nodes {
            Self::dfs_cycles(node, &self.edges, &mut visited, &mut path, &mut cycles);
        }
        cycles
    }

    fn dfs_cycles(
        node: &str,
        edges: &[(String, String)],
        visited: &mut std::collections::HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        if path.contains(&node.to_string()) {
            if let Some(idx) = path.iter().position(|x| x == node) {
                cycles.push(path[idx..].to_vec());
            }
            return;
        }
        if visited.contains(node) {
            return;
        }
        visited.insert(node.to_string());
        path.push(node.to_string());
        for (_, callee) in edges.iter().filter(|(s, _)| s == node) {
            Self::dfs_cycles(callee, edges, visited, path, cycles);
        }
        path.pop();
    }
}
