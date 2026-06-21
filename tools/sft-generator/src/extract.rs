//! AST extraction: turn a `.rs` file into structured [`ApiItem`]s using `syn`.
//!
//! Only genuinely public surface is collected (plus `#[test]` functions, which
//! are kept regardless of visibility because they are real, compiling usage).

use std::fs;
use std::path::{Path, PathBuf};

use quote::quote;
use syn::spanned::Spanned;
use syn::{
    Attribute, Expr, Fields, FnArg, ImplItem, Item, Lit, Meta, Pat, ReturnType, Signature, Type,
    UseTree, Visibility,
};
use walkdir::WalkDir;

use crate::model::{clean_code, ApiItem, Field, ItemKind, Param};

/// Maps a source file to the crate (package name) that owns it.
pub struct CrateMap {
    /// `(crate_dir, package_name)`, sorted longest-path-first so nested crates win.
    entries: Vec<(PathBuf, String)>,
}

impl CrateMap {
    pub fn build(root: &Path) -> Self {
        let mut entries = Vec::new();
        for e in WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() == "Cargo.toml")
        {
            if e.path().components().any(|c| c.as_os_str() == "target") {
                continue;
            }
            if let Ok(txt) = fs::read_to_string(e.path()) {
                if let Some(name) = parse_pkg_name(&txt) {
                    if let Some(dir) = e.path().parent() {
                        entries.push((dir.to_path_buf(), name));
                    }
                }
            }
        }
        entries.sort_by_key(|(p, _)| std::cmp::Reverse(p.components().count()));
        CrateMap { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn crate_for(&self, file: &Path) -> Option<&str> {
        self.entries
            .iter()
            .find(|(dir, _)| file.starts_with(dir))
            .map(|(_, n)| n.as_str())
    }
}

fn parse_pkg_name(txt: &str) -> Option<String> {
    let mut in_pkg = false;
    for line in txt.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_pkg = l == "[package]";
            continue;
        }
        if in_pkg && l.starts_with("name") {
            if let Some(eq) = l.find('=') {
                let v = l[eq + 1..]
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Per-file parsing context.
struct FileCtx<'a> {
    crate_name: &'a str,
    file: String,
    /// Owned source lines, used to slice real test bodies via span locations.
    lines: Vec<String>,
}

/// Result of parsing one file.
pub struct FileExtract {
    pub items: Vec<ApiItem>,
    pub reexports: Vec<String>,
    pub crate_name: String,
}

/// Parse a single source file. Returns `None` if `syn` cannot parse it.
pub fn extract_file(path: &Path, crate_name: &str) -> Option<FileExtract> {
    let src = fs::read_to_string(path).ok()?;
    let ast = syn::parse_file(&src).ok()?;
    let module_path = module_path_from_file(path);

    let ctx = FileCtx {
        crate_name,
        file: path.to_string_lossy().to_string(),
        lines: src.lines().map(|l| l.to_string()).collect(),
    };

    let mut items = Vec::new();
    let mut reexports = Vec::new();

    // Module / crate level `//!` doc.
    if let (Some(doc), examples) = extract_doc(&ast.attrs) {
        items.push(ApiItem {
            crate_name: crate_name.to_string(),
            module_path: module_path.clone(),
            name: if module_path.is_empty() {
                crate_name.to_string()
            } else {
                module_path.clone()
            },
            doc: Some(doc),
            doc_examples: examples,
            kind: ItemKind::ModuleDoc,
            file: ctx.file.clone(),
        });
    }

    walk_items(&ast.items, &module_path, &ctx, &mut items, &mut reexports);

    Some(FileExtract {
        items,
        reexports,
        crate_name: crate_name.to_string(),
    })
}

fn walk_items(
    items: &[Item],
    mod_path: &str,
    ctx: &FileCtx,
    out: &mut Vec<ApiItem>,
    reexports: &mut Vec<String>,
) {
    for item in items {
        match item {
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    let new_path = join_mod(mod_path, &m.ident.to_string());
                    walk_items(inner, &new_path, ctx, out, reexports);
                }
            }
            Item::Fn(f) => {
                if is_pub(&f.vis) {
                    out.push(make_fn(&f.attrs, &f.sig, None, mod_path, ctx));
                }
                if is_test(&f.attrs) {
                    if let Some(it) = make_test(&f.attrs, &f.sig, &f.block, mod_path, ctx) {
                        out.push(it);
                    }
                }
            }
            Item::Struct(s) if is_pub(&s.vis) => {
                let (doc, examples) = extract_doc(&s.attrs);
                let (fields, is_tuple, is_unit) = match &s.fields {
                    Fields::Named(n) => (
                        n.named
                            .iter()
                            .filter(|f| matches!(f.vis, Visibility::Public(_)))
                            .filter_map(|f| {
                                f.ident.as_ref().map(|id| Field {
                                    name: id.to_string(),
                                    ty: type_str(&f.ty),
                                })
                            })
                            .collect(),
                        false,
                        false,
                    ),
                    Fields::Unnamed(u) => (
                        u.unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, f)| Field {
                                name: i.to_string(),
                                ty: type_str(&f.ty),
                            })
                            .collect(),
                        true,
                        false,
                    ),
                    Fields::Unit => (Vec::new(), false, true),
                };
                out.push(ApiItem {
                    crate_name: ctx.crate_name.to_string(),
                    module_path: mod_path.to_string(),
                    name: s.ident.to_string(),
                    doc,
                    doc_examples: examples,
                    kind: ItemKind::Struct {
                        fields,
                        is_tuple,
                        is_unit,
                        derives: extract_derives(&s.attrs),
                    },
                    file: ctx.file.clone(),
                });
            }
            Item::Enum(e) if is_pub(&e.vis) => {
                let (doc, examples) = extract_doc(&e.attrs);
                out.push(ApiItem {
                    crate_name: ctx.crate_name.to_string(),
                    module_path: mod_path.to_string(),
                    name: e.ident.to_string(),
                    doc,
                    doc_examples: examples,
                    kind: ItemKind::Enum {
                        variants: e.variants.iter().map(|v| v.ident.to_string()).collect(),
                        derives: extract_derives(&e.attrs),
                    },
                    file: ctx.file.clone(),
                });
            }
            Item::Trait(t) if is_pub(&t.vis) => {
                let (doc, examples) = extract_doc(&t.attrs);
                let mut methods = Vec::new();
                let mut signatures = Vec::new();
                for ti in &t.items {
                    if let syn::TraitItem::Fn(m) = ti {
                        methods.push(m.sig.ident.to_string());
                        let sig = &m.sig;
                        signatures.push(clean_code(&quote!(#sig).to_string()));
                    }
                }
                out.push(ApiItem {
                    crate_name: ctx.crate_name.to_string(),
                    module_path: mod_path.to_string(),
                    name: t.ident.to_string(),
                    doc,
                    doc_examples: examples,
                    kind: ItemKind::Trait {
                        methods,
                        signatures,
                    },
                    file: ctx.file.clone(),
                });
            }
            Item::Impl(im) => {
                // Skip trait impls (`impl Trait for T`) — their methods are not new
                // API; keep inherent impls (`impl T`).
                if im.trait_.is_some() {
                    continue;
                }
                let parent = type_name(&im.self_ty);
                for it in &im.items {
                    if let ImplItem::Fn(m) = it {
                        if is_pub(&m.vis) {
                            out.push(make_fn(
                                &m.attrs,
                                &m.sig,
                                Some(parent.clone()),
                                mod_path,
                                ctx,
                            ));
                        }
                        if is_test(&m.attrs) {
                            if let Some(t) = make_test(&m.attrs, &m.sig, &m.block, mod_path, ctx) {
                                out.push(t);
                            }
                        }
                    }
                }
            }
            Item::Use(u) if is_pub(&u.vis) => {
                collect_use_leaves(&u.tree, reexports);
            }
            _ => {}
        }
    }
}

fn make_fn(
    attrs: &[Attribute],
    sig: &Signature,
    parent: Option<String>,
    mod_path: &str,
    ctx: &FileCtx,
) -> ApiItem {
    let (doc, examples) = extract_doc(attrs);
    let (receiver, params, ret, is_async) = parse_sig(sig);
    let signature = format!("pub {}", clean_code(&quote!(#sig).to_string()));
    ApiItem {
        crate_name: ctx.crate_name.to_string(),
        module_path: mod_path.to_string(),
        name: sig.ident.to_string(),
        doc,
        doc_examples: examples,
        kind: ItemKind::Function {
            is_async,
            receiver,
            params,
            ret,
            parent,
            signature,
        },
        file: ctx.file.clone(),
    }
}

fn make_test(
    attrs: &[Attribute],
    sig: &Signature,
    block: &syn::Block,
    mod_path: &str,
    ctx: &FileCtx,
) -> Option<ApiItem> {
    let start = attrs
        .iter()
        .map(|a| a.span().start().line)
        .min()
        .unwrap_or_else(|| sig.fn_token.span().start().line)
        .min(sig.fn_token.span().start().line);
    let end = block.span().end().line;
    if start == 0 || end == 0 || end < start || end > ctx.lines.len() {
        return None;
    }
    let body = dedent(&ctx.lines[start - 1..end]);
    if body.trim().is_empty() {
        return None;
    }
    Some(ApiItem {
        crate_name: ctx.crate_name.to_string(),
        module_path: mod_path.to_string(),
        name: sig.ident.to_string(),
        doc: None,
        doc_examples: Vec::new(),
        kind: ItemKind::Test { body },
        file: ctx.file.clone(),
    })
}

fn parse_sig(sig: &Signature) -> (Option<String>, Vec<Param>, Option<String>, bool) {
    let is_async = sig.asyncness.is_some();
    let mut receiver = None;
    let mut params = Vec::new();
    for input in &sig.inputs {
        match input {
            FnArg::Receiver(r) => {
                receiver = Some(clean_code(&quote!(#r).to_string()));
            }
            FnArg::Typed(pt) => {
                let name = match &*pt.pat {
                    Pat::Ident(pi) => pi.ident.to_string(),
                    _ => "arg".to_string(),
                };
                params.push(Param {
                    name,
                    ty: type_str(&pt.ty),
                });
            }
        }
    }
    let ret = match &sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, t) => Some(type_str(t)),
    };
    (receiver, params, ret, is_async)
}

fn type_str(ty: &Type) -> String {
    clean_code(&quote!(#ty).to_string())
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_else(|| type_str(ty)),
        _ => type_str(ty),
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .map(|s| s.ident == "test")
            .unwrap_or(false)
    })
}

fn extract_derives(attrs: &[Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.path().is_ident("derive") {
            if let Meta::List(list) = &a.meta {
                for part in list.tokens.to_string().split(',') {
                    let p = part.trim();
                    if !p.is_empty() {
                        out.push(p.rsplit("::").next().unwrap_or(p).trim().to_string());
                    }
                }
            }
        }
    }
    out
}

/// Returns the joined doc text and any fenced ```rust code blocks within it.
fn extract_doc(attrs: &[Attribute]) -> (Option<String>, Vec<String>) {
    let mut lines = Vec::new();
    for a in attrs {
        if a.path().is_ident("doc") {
            if let Meta::NameValue(nv) = &a.meta {
                if let Expr::Lit(syn::ExprLit {
                    lit: Lit::Str(s), ..
                }) = &nv.value
                {
                    let v = s.value();
                    lines.push(v.strip_prefix(' ').unwrap_or(&v).to_string());
                }
            }
        }
    }
    if lines.is_empty() {
        return (None, Vec::new());
    }
    let joined = lines.join("\n");
    let examples = extract_code_blocks(&joined);
    (Some(joined), examples)
}

fn extract_code_blocks(doc: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut cur: Vec<String> = Vec::new();
    for line in doc.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            if in_block {
                if !cur.is_empty() {
                    blocks.push(cur.join("\n"));
                }
                cur.clear();
                in_block = false;
            } else {
                in_block = true;
            }
            continue;
        }
        if in_block {
            // Drop hidden doctest lines (`# ...`).
            if t == "#" || t.starts_with("# ") {
                continue;
            }
            cur.push(line.to_string());
        }
    }
    blocks
}

fn collect_use_leaves(tree: &UseTree, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(p) => collect_use_leaves(&p.tree, out),
        UseTree::Name(n) => out.push(n.ident.to_string()),
        UseTree::Rename(r) => out.push(r.rename.to_string()),
        UseTree::Group(g) => {
            for t in &g.items {
                collect_use_leaves(t, out);
            }
        }
        UseTree::Glob(_) => {}
    }
}

fn module_path_from_file(file: &Path) -> String {
    let comps: Vec<String> = file
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if let Some(src_idx) = comps.iter().rposition(|c| c == "src") {
        let mut parts: Vec<String> = comps[src_idx + 1..].to_vec();
        if let Some(last) = parts.last().cloned() {
            if last == "lib.rs" || last == "main.rs" || last == "mod.rs" {
                parts.pop();
            } else if let Some(stripped) = last.strip_suffix(".rs") {
                *parts.last_mut().unwrap() = stripped.to_string();
            }
        }
        parts.join("::")
    } else {
        String::new()
    }
}

fn join_mod(base: &str, child: &str) -> String {
    if base.is_empty() {
        child.to_string()
    } else {
        format!("{base}::{child}")
    }
}

/// Strip the common leading indentation from a block of lines, cap very long
/// bodies, and join with newlines.
fn dedent(lines: &[String]) -> String {
    const MAX: usize = 120;
    let slice = if lines.len() > MAX {
        &lines[..MAX]
    } else {
        lines
    };
    let indent = slice
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut body: Vec<String> = slice
        .iter()
        .map(|l| {
            if l.len() >= indent {
                l[indent..].to_string()
            } else {
                l.clone()
            }
        })
        .collect();
    if lines.len() > MAX {
        body.push("// ...".to_string());
    }
    body.join("\n")
}
