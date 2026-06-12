//! Extraction des items publics Rust via `syn`.

use crate::scanner::{ItemKind, PublicItem};
use anyhow::Result;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{
    File, ImplItem, Item, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemStatic,
    ItemStruct, ItemTrait, Visibility,
};

/// Parse tous les fichiers Rust passés et renvoie les items `pub`.
pub fn parse_project(project_name: &str, files: &[PathBuf]) -> Result<Vec<PublicItem>> {
    let mut out = Vec::new();
    for f in files {
        if let Some(items) = parse_file(project_name, f) {
            out.extend(items);
        }
    }
    Ok(out)
}

fn parse_file(project: &str, path: &Path) -> Option<Vec<PublicItem>> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_file_text(project, path, &text)
}

pub fn parse_file_text(project: &str, path: &Path, text: &str) -> Option<Vec<PublicItem>> {
    let ast: File = syn::parse_file(text).ok()?;
    let mut v = Visitor {
        project: project.to_string(),
        path: path.to_path_buf(),
        text,
        items: Vec::new(),
    };
    v.visit_file(&ast);
    Some(v.items)
}

struct Visitor<'a> {
    project: String,
    path: PathBuf,
    text: &'a str,
    items: Vec<PublicItem>,
}

impl<'a> Visitor<'a> {
    fn line_of(&self, byte_offset: usize) -> usize {
        self.text[..byte_offset.min(self.text.len())]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1
    }

    fn is_pub(vis: &Visibility) -> bool {
        matches!(vis, Visibility::Public(_))
    }

    fn push(&mut self, kind: ItemKind, name: String, signature: String, line: usize, body_text: &str, doc: Option<String>) {
        let body_tokens = tokenize(body_text);
        self.items.push(PublicItem {
            project: self.project.clone(),
            file: self.path.clone(),
            line,
            kind,
            name,
            signature,
            body_tokens,
            doc,
        });
    }
}

impl<'ast, 'a> Visit<'ast> for Visitor<'a> {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Fn(ItemFn { vis, sig, block, attrs, .. }) if Self::is_pub(vis) => {
                let kind = if sig.asyncness.is_some() { ItemKind::AsyncFn } else { ItemKind::Fn };
                let line = self.line_of(0);
                let signature = quote::quote!(#sig).to_string();
                let body = quote::quote!(#block).to_string();
                let doc = extract_doc(attrs);
                self.push(kind, sig.ident.to_string(), signature, line, &body, doc);
            }
            Item::Struct(ItemStruct { vis, ident, attrs, .. }) if Self::is_pub(vis) => {
                let line = self.line_of(0);
                let doc = extract_doc(attrs);
                self.push(ItemKind::Struct, ident.to_string(), format!("pub struct {}", ident), line, "", doc);
            }
            Item::Enum(ItemEnum { vis, ident, attrs, .. }) if Self::is_pub(vis) => {
                let line = self.line_of(0);
                let doc = extract_doc(attrs);
                self.push(ItemKind::Enum, ident.to_string(), format!("pub enum {}", ident), line, "", doc);
            }
            Item::Trait(ItemTrait { vis, ident, attrs, .. }) if Self::is_pub(vis) => {
                let line = self.line_of(0);
                let doc = extract_doc(attrs);
                self.push(ItemKind::Trait, ident.to_string(), format!("pub trait {}", ident), line, "", doc);
            }
            Item::Impl(ItemImpl { items, self_ty, .. }) => {
                let ty_str = quote::quote!(#self_ty).to_string();
                for ii in items {
                    if let ImplItem::Fn(m) = ii {
                        if Self::is_pub(&m.vis) {
                            let kind = if m.sig.asyncness.is_some() { ItemKind::AsyncFn } else { ItemKind::Fn };
                            let sig = &m.sig;
                            let body = &m.block;
                            let name = format!("{}::{}", ty_str.trim(), sig.ident);
                            let signature = format!("impl {} {{ {} }}", ty_str.trim(), quote::quote!(#sig));
                            let body_str = quote::quote!(#body).to_string();
                            let doc = extract_doc(&m.attrs);
                            self.push(kind, name, signature, self.line_of(0), &body_str, doc);
                        }
                    }
                }
            }
            Item::Const(ItemConst { vis, ident, attrs, .. }) if Self::is_pub(vis) => {
                let doc = extract_doc(attrs);
                self.push(ItemKind::Const, ident.to_string(), format!("pub const {}", ident), self.line_of(0), "", doc);
            }
            Item::Static(ItemStatic { vis, ident, attrs, .. }) if Self::is_pub(vis) => {
                let doc = extract_doc(attrs);
                self.push(ItemKind::Static, ident.to_string(), format!("pub static {}", ident), self.line_of(0), "", doc);
            }
            _ => {}
        }
        syn::visit::visit_item(self, item);
    }
}

fn extract_doc(attrs: &[syn::Attribute]) -> Option<String> {
    let mut lines = Vec::new();
    for a in attrs {
        if a.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &a.meta {
                if let syn::Expr::Lit(lit) = &nv.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        lines.push(s.value().trim().to_string());
                    }
                }
            }
        }
    }
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for c in text.chars() {
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
