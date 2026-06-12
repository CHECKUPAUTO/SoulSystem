use std::collections::BTreeMap;

use rustpython_ast::text_size::TextRange;
use rustpython_ast::{Expr, Stmt, Suite, Visitor};
use rustpython_parser::Parse;

/// A histogram mapping AST node kind names to their occurrence count.
pub type NodeHist = BTreeMap<String, u32>;

/// Aggregated fingerprint from one or more Python source files.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Fingerprint {
    pub node_hist: NodeHist,
    #[serde(with = "edge_map_serde")]
    pub edges: BTreeMap<(String, String), u32>,
    pub depth_profiles: Vec<(u32, f32, f32)>,
}

/// Custom serde for `BTreeMap<(String, String), u32>` — JSON requires string keys.
mod edge_map_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S>(edges: &BTreeMap<(String, String), u32>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<(&(String, String), &u32)> = edges.iter().collect();
        entries.serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<BTreeMap<(String, String), u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<((String, String), u32)> = Vec::deserialize(d)?;
        Ok(entries.into_iter().collect())
    }
}

impl Fingerprint {
    pub fn merge(&mut self, other: &Self) {
        for (k, v) in &other.node_hist {
            *self.node_hist.entry(k.clone()).or_insert(0) += v;
        }
        for (k, v) in &other.edges {
            *self.edges.entry(k.clone()).or_insert(0) += v;
        }
        self.depth_profiles.extend_from_slice(&other.depth_profiles);
    }
}

/// Fingerprint a single Python module.
///
/// # Errors
/// Returns [`AntiCloneError::Parse`] if the source fails to parse.
pub fn fingerprint_module(source: &str) -> Result<Fingerprint, crate::error::AntiCloneError> {
    let suite = Suite::parse_without_path(source)
        .map_err(|e| crate::error::AntiCloneError::Parse(e.to_string()))?;

    let node_hist = node_histogram(&suite);
    let edges = crate::edges::extract_edges(&suite);
    let depth_profiles = crate::depth::collect_depth_profiles(&suite);

    Ok(Fingerprint {
        node_hist,
        edges,
        depth_profiles,
    })
}

// ── Node histogram via Visitor ──────────────────────────────────────

fn node_histogram(suite: &[Stmt<TextRange>]) -> NodeHist {
    let mut collector = HistCollector::default();
    for stmt in suite {
        collector.visit_stmt(stmt.clone());
    }
    collector.hist
}

#[derive(Default)]
struct HistCollector {
    hist: NodeHist,
}

impl Visitor<TextRange> for HistCollector {
    fn visit_stmt(&mut self, node: Stmt<TextRange>) {
        let name = match &node {
            Stmt::FunctionDef(_) => "StmtFunctionDef",
            Stmt::AsyncFunctionDef(_) => "StmtAsyncFunctionDef",
            Stmt::ClassDef(_) => "StmtClassDef",
            Stmt::Return(_) => "StmtReturn",
            Stmt::Delete(_) => "StmtDelete",
            Stmt::Assign(_) => "StmtAssign",
            Stmt::TypeAlias(_) => "StmtTypeAlias",
            Stmt::AugAssign(_) => "StmtAugAssign",
            Stmt::AnnAssign(_) => "StmtAnnAssign",
            Stmt::For(_) => "StmtFor",
            Stmt::AsyncFor(_) => "StmtAsyncFor",
            Stmt::While(_) => "StmtWhile",
            Stmt::If(_) => "StmtIf",
            Stmt::With(_) => "StmtWith",
            Stmt::AsyncWith(_) => "StmtAsyncWith",
            Stmt::Match(_) => "StmtMatch",
            Stmt::Raise(_) => "StmtRaise",
            Stmt::Try(_) => "StmtTry",
            Stmt::TryStar(_) => "StmtTryStar",
            Stmt::Assert(_) => "StmtAssert",
            Stmt::Import(_) => "StmtImport",
            Stmt::ImportFrom(_) => "StmtImportFrom",
            Stmt::Global(_) => "StmtGlobal",
            Stmt::Nonlocal(_) => "StmtNonlocal",
            Stmt::Expr(_) => "StmtExpr",
            Stmt::Pass(_) => "StmtPass",
            Stmt::Break(_) => "StmtBreak",
            Stmt::Continue(_) => "StmtContinue",
        };
        *self.hist.entry(name.to_string()).or_insert(0) += 1;
        self.generic_visit_stmt(node);
    }

    fn visit_expr(&mut self, node: Expr<TextRange>) {
        let name = match &node {
            Expr::BoolOp(_) => "ExprBoolOp",
            Expr::NamedExpr(_) => "ExprNamedExpr",
            Expr::BinOp(_) => "ExprBinOp",
            Expr::UnaryOp(_) => "ExprUnaryOp",
            Expr::Lambda(_) => "ExprLambda",
            Expr::IfExp(_) => "ExprIfExp",
            Expr::Dict(_) => "ExprDict",
            Expr::Set(_) => "ExprSet",
            Expr::ListComp(_) => "ExprListComp",
            Expr::SetComp(_) => "ExprSetComp",
            Expr::DictComp(_) => "ExprDictComp",
            Expr::GeneratorExp(_) => "ExprGeneratorExp",
            Expr::Await(_) => "ExprAwait",
            Expr::Yield(_) => "ExprYield",
            Expr::YieldFrom(_) => "ExprYieldFrom",
            Expr::Compare(_) => "ExprCompare",
            Expr::Call(_) => "ExprCall",
            Expr::FormattedValue(_) => "ExprFormattedValue",
            Expr::JoinedStr(_) => "ExprJoinedStr",
            Expr::Constant(_) => "ExprConstant",
            Expr::Attribute(_) => "ExprAttribute",
            Expr::Subscript(_) => "ExprSubscript",
            Expr::Starred(_) => "ExprStarred",
            Expr::Name(_) => "ExprName",
            Expr::List(_) => "ExprList",
            Expr::Tuple(_) => "ExprTuple",
            Expr::Slice(_) => "ExprSlice",
        };
        *self.hist.entry(name.to_string()).or_insert(0) += 1;
        self.generic_visit_expr(node);
    }
}

/// Fingerprint arbitrary text via SHA-256 (hex).
///
/// Used for deduplication of non-Python content such as URLs.
#[must_use]
pub fn fingerprint_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(text.as_bytes());
    hex::encode(hash)
}
