//! Détecteur api_surface : producteurs/consommateurs/transformers sur des
//! stems communs entre projets différents.

use crate::analyzer::{auto_score_from, priority_from_score, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::HashMap;

const PRODUCERS: &[&str] = &[
    "encode",
    "serialize",
    "build",
    "create",
    "generate",
    "make",
    "compile",
    "embed",
    "store",
    "save",
    "produce",
    "emit",
    "render",
    "compress",
    "publish",
    "send",
    "push",
];
const CONSUMERS: &[&str] = &[
    "decode",
    "deserialize",
    "parse",
    "apply",
    "run",
    "execute",
    "load",
    "consume",
    "subscribe",
    "receive",
    "fetch",
    "pull",
    "decompress",
    "interpret",
    "evaluate",
    "process",
];
const TRANSFORMERS: &[&str] = &[
    "transform",
    "convert",
    "translate",
    "map",
    "filter",
    "normalize",
    "validate",
    "sanitize",
    "enrich",
    "aggregate",
];

#[derive(Default)]
pub struct ApiSurfaceDetector;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Role {
    Producer,
    Consumer,
    Transformer,
}

fn classify(name: &str) -> Option<(Role, String)> {
    let lower = name.to_ascii_lowercase();
    for verb in PRODUCERS {
        if lower.starts_with(verb) || lower.contains(&format!("_{}", verb)) || lower.ends_with(verb)
        {
            return Some((Role::Producer, stem_of(&lower, verb)));
        }
    }
    for verb in CONSUMERS {
        if lower.starts_with(verb) || lower.contains(&format!("_{}", verb)) || lower.ends_with(verb)
        {
            return Some((Role::Consumer, stem_of(&lower, verb)));
        }
    }
    for verb in TRANSFORMERS {
        if lower.starts_with(verb) || lower.contains(&format!("_{}", verb)) || lower.ends_with(verb)
        {
            return Some((Role::Transformer, stem_of(&lower, verb)));
        }
    }
    None
}

fn stem_of(name: &str, verb: &str) -> String {
    let stripped = name.replace(verb, "").replace('_', "").replace('-', "");
    stripped.trim_matches('_').to_string()
}

impl Detector for ApiSurfaceDetector {
    fn name(&self) -> &'static str {
        "api_surface"
    }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        struct Slot {
            project: String,
            role: Role,
            stem: String,
            item_idx: usize,
        }
        let mut slots: Vec<Slot> = Vec::new();
        for (proj, items) in ctx.public_items.iter() {
            for (idx, it) in items.iter().enumerate() {
                if !it.kind.is_callable() {
                    continue;
                }
                if let Some((role, stem)) = classify(&it.name) {
                    if stem.len() < 3 {
                        continue;
                    }
                    slots.push(Slot {
                        project: proj.clone(),
                        role,
                        stem,
                        item_idx: idx,
                    });
                }
            }
        }

        let mut by_stem: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, s) in slots.iter().enumerate() {
            by_stem.entry(s.stem.clone()).or_default().push(i);
        }

        let mut out = Vec::new();
        for (stem, group) in by_stem {
            if group.len() < 2 {
                continue;
            }
            // Pour chaque paire de slots (i, j) inter-projets et rôles complémentaires.
            for i in 0..group.len() {
                for j in (i + 1)..group.len() {
                    let a = &slots[group[i]];
                    let b = &slots[group[j]];
                    if a.project == b.project {
                        continue;
                    }
                    let (ty, eff) = match (a.role, b.role) {
                        (Role::Producer, Role::Consumer) | (Role::Consumer, Role::Producer) => {
                            ("ApiComplementarity", "easy")
                        }
                        (Role::Producer, Role::Transformer)
                        | (Role::Transformer, Role::Producer)
                        | (Role::Transformer, Role::Consumer)
                        | (Role::Consumer, Role::Transformer) => ("DataPipeline", "medium"),
                        _ => continue,
                    };
                    let id = Synergy::stable_id(ty, &a.project, &b.project, &stem);
                    let auto = auto_score_from(2, 0.85, type_floor(ty));
                    let (item_a, item_b) = (
                        ctx.public_items
                            .get(&a.project)
                            .and_then(|v| v.get(a.item_idx)),
                        ctx.public_items
                            .get(&b.project)
                            .and_then(|v| v.get(b.item_idx)),
                    );
                    let (Some(ia), Some(ib)) = (item_a, item_b) else {
                        continue;
                    };

                    out.push(Synergy {
                        id: id.clone(),
                        synergy_type: ty.into(),
                        source_project: a.project.clone(),
                        target_project: b.project.clone(),
                        description: format!(
                            "Surface API complémentaire sur `{}` : `{}::{}` ({:?}) ↔ `{}::{}` ({:?})",
                            stem, a.project, ia.name, a.role, b.project, ib.name, b.role
                        ),
                        priority: priority_from_score(auto),
                        effort: eff.into(),
                        value: "medium".into(),
                        auto_score: auto,
                        adjusted_score: auto,
                        evidence: vec![
                            Evidence {
                                project: a.project.clone(),
                                path: ia.file.clone(),
                                line: Some(ia.line),
                                fragment: format!("{:?} {}", a.role, ia.name),
                                weight: 0.85,
                            },
                            Evidence {
                                project: b.project.clone(),
                                path: ib.file.clone(),
                                line: Some(ib.line),
                                fragment: format!("{:?} {}", b.role, ib.name),
                                weight: 0.85,
                            },
                        ],
                        suggested_action: Some(format!(
                            "Chaîner `{a}::{n_a}` → `{b}::{n_b}` ; exposer une API `pipeline/{stem}`",
                            a = a.project, n_a = ia.name, b = b.project, n_b = ib.name, stem = stem
                        )),
                        fingerprint: blake3::hash(format!("api:{}:{}", stem, id).as_bytes()).to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        out.sort_by(|x, y| {
            y.auto_score
                .partial_cmp(&x.auto_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(80);
        Ok(out)
    }
}
