//! Cross-crate and multi-step *scenario* pairs.
//!
//! Unlike the per-item pairs in [`crate::templates`], these are **compositions**:
//! they show how real symbols fit together. Everything referenced (crate names,
//! struct names, method names, `new` constructors) is taken from the extracted
//! AST, so the building blocks are real — only the narrative gluing them is
//! synthesized. Tagged `scenario.*` so they can be filtered or weighted.

use std::collections::HashMap;

use crate::extract::FileExtract;
use crate::model::{crate_ident, ItemKind, Pair};

struct MethodRef {
    name: String,
    args: String,
    is_async: bool,
}

struct StructRef {
    name: String,
    module: String,
    has_new: bool,
    new_args: String,
    methods: Vec<MethodRef>,
}

#[derive(Default)]
struct CrateInfo {
    structs: HashMap<String, StructRef>,
    order: Vec<String>,
}

pub struct Index {
    crates: HashMap<String, CrateInfo>,
}

impl Index {
    pub fn build(extracts: &[FileExtract]) -> Index {
        let mut crates: HashMap<String, CrateInfo> = HashMap::new();
        for ex in extracts {
            for it in &ex.items {
                let ci = crates.entry(it.crate_name.clone()).or_default();
                match &it.kind {
                    ItemKind::Struct { .. } => {
                        let entry =
                            ci.structs
                                .entry(it.name.clone())
                                .or_insert_with(|| StructRef {
                                    name: it.name.clone(),
                                    module: it.module_path.clone(),
                                    has_new: false,
                                    new_args: String::new(),
                                    methods: Vec::new(),
                                });
                        if entry.module.is_empty() {
                            entry.module = it.module_path.clone();
                        }
                        if !ci.order.contains(&it.name) {
                            ci.order.push(it.name.clone());
                        }
                    }
                    ItemKind::Function {
                        parent: Some(p),
                        receiver,
                        params,
                        is_async,
                        ..
                    } => {
                        let entry = ci.structs.entry(p.clone()).or_insert_with(|| StructRef {
                            name: p.clone(),
                            module: it.module_path.clone(),
                            has_new: false,
                            new_args: String::new(),
                            methods: Vec::new(),
                        });
                        let args = params
                            .iter()
                            .map(|x| x.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if receiver.is_none()
                            && (it.name == "new"
                                || it.name == "default"
                                || it.name == "with_config")
                        {
                            entry.has_new = true;
                            if entry.new_args.is_empty() {
                                entry.new_args = args;
                            }
                        } else if receiver.is_some() && entry.methods.len() < 4 {
                            entry.methods.push(MethodRef {
                                name: it.name.clone(),
                                args,
                                is_async: *is_async,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        Index { crates }
    }

    /// Primary types of a crate: those with a `new`-like constructor first.
    fn primary(&self, c: &str) -> Vec<&StructRef> {
        let Some(ci) = self.crates.get(c) else {
            return Vec::new();
        };
        let mut v: Vec<&StructRef> = ci.order.iter().filter_map(|n| ci.structs.get(n)).collect();
        v.sort_by_key(|s| !s.has_new); // has_new == true first
        v.truncate(4);
        v
    }

    pub fn pairs(&self) -> Vec<(Pair, String)> {
        let mut out = Vec::new();
        self.workflow_pairs(&mut out);
        self.subsystem_pairs(&mut out);
        out
    }

    /// Intra-crate multi-step workflows: build an instance, then chain real methods.
    fn workflow_pairs(&self, out: &mut Vec<(Pair, String)>) {
        let mut crate_names: Vec<&String> = self.crates.keys().collect();
        crate_names.sort();
        for c in crate_names {
            let ci = &self.crates[c];
            for sname in &ci.order {
                let s = &ci.structs[sname];
                if !s.has_new || s.methods.len() < 2 {
                    continue;
                }
                let import = if s.module.is_empty() {
                    format!("{}::{}", crate_ident(c), s.name)
                } else {
                    format!("{}::{}::{}", crate_ident(c), s.module, s.name)
                };
                let any_async = s.methods.iter().any(|m| m.is_async);
                let mut body = format!("let mut instance = {}::new({});\n", s.name, s.new_args);
                for m in &s.methods {
                    let awaitp = if m.is_async { ".await" } else { "" };
                    body.push_str(&format!(
                        "let _ = instance.{}({}){};\n",
                        m.name, m.args, awaitp
                    ));
                }
                let body = body.trim_end();
                let code = if any_async {
                    format!("async fn workflow() {{\n{}\n}}", indent4(body))
                } else {
                    body.to_string()
                };
                let methods_list = s
                    .methods
                    .iter()
                    .map(|m| format!("`{}`", m.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push((
                    Pair {
                        user: format!(
                            "Montre un workflow complet avec `{}` du crate `{c}` : construction puis enchaînement des méthodes.",
                            s.name
                        ),
                        assistant: format!(
                            "On construit l'instance avec `new`, puis on appelle {methods_list} :\n\n```rust\nuse {import};\n\n{code}\n```"
                        ),
                        source: "scenario.workflow",
                    },
                    c.clone(),
                ));
            }
        }
    }

    /// Inter-crate integration within a subsystem, plus a subsystem overview.
    fn subsystem_pairs(&self, out: &mut Vec<(Pair, String)>) {
        // Group crates (that have primary types) by subsystem.
        let mut groups: HashMap<&'static str, Vec<String>> = HashMap::new();
        let mut names: Vec<&String> = self.crates.keys().collect();
        names.sort();
        for c in names {
            if self.primary(c).is_empty() {
                continue;
            }
            if let Some(sub) = subsystem(c) {
                groups.entry(sub).or_default().push(c.clone());
            }
        }

        let mut subs: Vec<&'static str> = groups.keys().copied().collect();
        subs.sort();
        for sub in subs {
            let crates = &groups[sub];

            // Overview of the subsystem.
            let mut overview = format!(
                "Le sous-système **{sub}** regroupe notamment ces crates et leurs types clés :\n\n"
            );
            for c in crates.iter().take(12) {
                let prim = self.primary(c);
                let types = prim
                    .iter()
                    .map(|s| format!("`{}`", s.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                overview.push_str(&format!("- `{c}` : {types}\n"));
            }
            out.push((
                Pair {
                    user: format!(
                        "Quels crates composent le sous-système {sub} de SoulSystem et quels sont leurs types clés ?"
                    ),
                    assistant: overview.trim_end().to_string(),
                    source: "scenario.subsystem",
                },
                crates[0].clone(),
            ));

            // Pairwise integration of adjacent crates (sliding window of 2).
            for w in crates.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                let (pa, pb) = (self.primary(a), self.primary(b));
                let (Some(ta), Some(tb)) = (pa.first(), pb.first()) else {
                    continue;
                };
                let ca = ctor_line(ta, "producteur");
                let cb = ctor_line(tb, "consommateur");
                out.push((
                    Pair {
                        user: format!(
                            "Dans le sous-système {sub}, comment faire interagir les crates `{a}` et `{b}` ?"
                        ),
                        assistant: format!(
                            "Types clés :\n- `{a}` : `{}`\n- `{b}` : `{}`\n\nEsquisse d'intégration :\n```rust\nuse {}::{};\nuse {}::{};\n\n{ca}\n{cb}\n// `{a}` produit des données consommées par `{b}`\n```",
                            ta.name,
                            tb.name,
                            crate_ident(a),
                            ta.name,
                            crate_ident(b),
                            tb.name,
                        ),
                        source: "scenario.pair",
                    },
                    a.clone(),
                ));
            }
        }
    }
}

fn ctor_line(s: &StructRef, var: &str) -> String {
    if s.has_new {
        format!("let {var} = {}::new({});", s.name, s.new_args)
    } else {
        format!(
            "let {var}: {} = /* construire une instance */ todo!();",
            s.name
        )
    }
}

/// Map a crate name to its subsystem label by prefix.
fn subsystem(name: &str) -> Option<&'static str> {
    let n = name.replace('_', "-");
    if n.starts_with("scirust-trading") {
        Some("SciRust Trading")
    } else if n.starts_with("scirust") {
        Some("SciRust (calcul scientifique)")
    } else if n.starts_with("soullink") {
        Some("SoulLink (réseau neuronal)")
    } else if n.starts_with("avid") {
        Some("AVID")
    } else if n.starts_with("soul-") || n.starts_with("soul") {
        Some("Entité autonome / infrastructure Soul")
    } else {
        None
    }
}

fn indent4(s: &str) -> String {
    s.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("    {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
