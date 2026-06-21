//! Data model for items extracted from the SoulSystem workspace.
//!
//! Everything here is a faithful, structured view of *real* source: signatures,
//! doc comments, fields, variants and test bodies are pulled from the AST so the
//! generated SFT pairs stay grounded in the actual API rather than invented.

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    Function {
        is_async: bool,
        receiver: Option<String>,
        params: Vec<Param>,
        ret: Option<String>,
        /// Owning type when this is an inherent/`impl` method.
        parent: Option<String>,
        /// Full reconstructed signature, e.g. `pub async fn foo(&self) -> Result<T, E>`.
        signature: String,
        derives_default_parent: bool,
    },
    Struct {
        fields: Vec<Field>,
        is_tuple: bool,
        is_unit: bool,
        derives: Vec<String>,
    },
    Enum {
        variants: Vec<String>,
        derives: Vec<String>,
    },
    Trait {
        methods: Vec<String>,
        /// Full reconstructed signatures of the required methods, e.g.
        /// `fn device_name(&self) -> String`.
        signatures: Vec<String>,
    },
    /// A real `#[test]` / `#[tokio::test]` function — the body is sliced verbatim
    /// from source so it is genuine, compiling usage.
    Test { is_async: bool, body: String },
    /// Crate / module level `//!` documentation.
    ModuleDoc,
}

#[derive(Debug, Clone)]
pub struct ApiItem {
    pub crate_name: String,
    /// Module path inside the crate, e.g. `client::session` (may be empty for root).
    pub module_path: String,
    pub name: String,
    pub doc: Option<String>,
    /// Fenced ```` ```rust ```` code blocks lifted from the doc comment.
    pub doc_examples: Vec<String>,
    pub kind: ItemKind,
    pub file: String,
}

impl ApiItem {
    /// Fully-qualified path used in `use` statements and prose.
    pub fn full_path(&self) -> String {
        match (self.module_path.is_empty(), &self.kind) {
            (_, ItemKind::ModuleDoc) if self.module_path.is_empty() => self.crate_name.clone(),
            (_, ItemKind::ModuleDoc) => format!("{}::{}", self.crate_name, self.module_path),
            (true, _) => format!("{}::{}", self.crate_name, self.name),
            (false, _) => format!("{}::{}::{}", self.crate_name, self.module_path, self.name),
        }
    }

    /// Best-effort `use` path. Re-exports often flatten the module path, but the
    /// fully-qualified form is always valid.
    pub fn use_path(&self) -> String {
        self.full_path()
    }

    /// First sentence / first non-empty line of the doc comment, for one-liners.
    pub fn doc_summary(&self) -> Option<String> {
        let doc = self.doc.as_ref()?;
        let line = doc
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("```"))?;
        Some(line.trim_end_matches('.').to_string())
    }
}

/// A single generated training example (one user turn + one assistant turn).
#[derive(Debug, Clone)]
pub struct Pair {
    pub user: String,
    pub assistant: String,
    /// Coarse provenance tag, recorded as metadata, useful for filtering/weighting.
    pub source: &'static str,
}

/// Collapse the spacing that `quote` inserts around punctuation so reconstructed
/// types and signatures read like hand-written Rust (`Result<T, E>`, not
/// `Result < T , E >`).
pub fn clean_code(s: &str) -> String {
    let mut out = s.to_string();
    // Order matters: collapse `::` before single `:`, and never glue the `->`
    // return arrow to the type that follows it.
    for (from, to) in [
        (" :: ", "::"),
        (":: ", "::"),
        (" ::", "::"),
        (" < ", "<"),
        ("< ", "<"),
        (" <", "<"),
        (" > ", ">"),
        (" >", ">"),
        (" : ", ": "),
        (" :", ":"),
        ("& ", "&"),
        ("( ", "("),
        (" (", "("),
        (" )", ")"),
        ("[ ", "["),
        (" ]", "]"),
        (" ;", ";"),
        (" !", "!"),
        (" ,", ","),
    ] {
        out = out.replace(from, to);
    }
    // Normalise comma spacing, then tidy trailing commas before `)`/`>`.
    out = out.replace(",", ", ");
    out = out.replace(",  ", ", ");
    out = out.replace(", )", ")");
    out = out.replace(", >", ">");
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out.trim().to_string()
}
