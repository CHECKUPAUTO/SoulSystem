//! Détecteur lang_bridge : repère les fonctions du même nom (normalisé)
//! exposées en Rust ET en Python — candidates à port (PyO3/maturin).

use crate::analyzer::{auto_score_from, priority_from_score, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::scanner::ItemKind;
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Default)]
pub struct LangBridgeDetector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang { Rust, Python }

fn lang_of(kind: ItemKind) -> Option<Lang> {
    match kind {
        ItemKind::Fn | ItemKind::AsyncFn => Some(Lang::Rust),
        ItemKind::PyFunction | ItemKind::PyMethod => Some(Lang::Python),
        _ => None,
    }
}

fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

impl Detector for LangBridgeDetector {
    fn name(&self) -> &'static str { "lang_bridge" }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let mut by_name: HashMap<String, Vec<(String, Lang, usize)>> = HashMap::new();
        for (proj, items) in ctx.public_items.iter() {
            for (idx, it) in items.iter().enumerate() {
                let Some(l) = lang_of(it.kind) else { continue; };
                let key = normalize(&it.name);
                if key.len() < 5 { continue; }
                if key.starts_with("__") && key.ends_with("__") { continue; }
                if matches!(key.as_str(), "new" | "from" | "into" | "main" | "init" | "default" | "build" | "run") {
                    continue;
                }
                by_name.entry(key).or_default().push((proj.clone(), l, idx));
            }
        }

        let mut out = Vec::new();
        for (name, occurs) in by_name {
            // Paires Rust ↔ Python inter-projets.
            for i in 0..occurs.len() {
                for j in (i + 1)..occurs.len() {
                    let (proj_a, lang_a, idx_a) = &occurs[i];
                    let (proj_b, lang_b, idx_b) = &occurs[j];
                    if lang_a == lang_b { continue; }

                    let id = Synergy::stable_id("LanguageBridge", proj_a, proj_b, &name);
                    let auto = auto_score_from(2, 0.85, type_floor("LanguageBridge"));
                    let (item_a, item_b) = (
                        ctx.public_items.get(proj_a).and_then(|v| v.get(*idx_a)),
                        ctx.public_items.get(proj_b).and_then(|v| v.get(*idx_b)),
                    );
                    let (Some(ia), Some(ib)) = (item_a, item_b) else { continue };
                    out.push(Synergy {
                        id: id.clone(),
                        synergy_type: "LanguageBridge".into(),
                        source_project: proj_a.clone(),
                        target_project: proj_b.clone(),
                        description: format!(
                            "`{}` existe en {:?} dans `{}` et en {:?} dans `{}` — bridge PyO3 candidat",
                            name, lang_a, proj_a, lang_b, proj_b
                        ),
                        priority: priority_from_score(auto + 0.2),
                        effort: "medium".into(),
                        value: "high".into(),
                        auto_score: auto,
                        adjusted_score: auto,
                        evidence: vec![
                            Evidence {
                                project: proj_a.clone(),
                                path: ia.file.clone(),
                                line: Some(ia.line),
                                fragment: format!("[{:?}] {} {}", lang_a, ia.kind_label(), ia.name),
                                weight: 0.85,
                            },
                            Evidence {
                                project: proj_b.clone(),
                                path: ib.file.clone(),
                                line: Some(ib.line),
                                fragment: format!("[{:?}] {} {}", lang_b, ib.kind_label(), ib.name),
                                weight: 0.85,
                            },
                        ],
                        suggested_action: Some(format!(
                            "1. Comparer les deux impls de `{}`\n2. Si Rust complet → binding PyO3, déprécier Python\n3. Sinon → porter le Python en Rust",
                            name
                        )),
                        fingerprint: blake3::hash(format!("lb:{}:{}", name, id).as_bytes()).to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        out.sort_by(|x, y| y.auto_score.partial_cmp(&x.auto_score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(40);
        Ok(out)
    }
}
