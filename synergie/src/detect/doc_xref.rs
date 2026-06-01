//! Détecteur doc_xref : READMEs qui mentionnent d'autres projets.

use crate::analyzer::{auto_score_from, priority_from_score, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::types::{Evidence, Language, Synergy};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct DocXrefDetector;

impl Detector for DocXrefDetector {
    fn name(&self) -> &'static str {
        "doc_xref"
    }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let mut docs_by_project: HashMap<String, Vec<(PathBuf, String)>> = HashMap::new();
        for (proj, idx) in ctx.file_index.iter() {
            let mut local = Vec::new();
            for p in &idx.readmes {
                if let Ok(text) = std::fs::read_to_string(p) {
                    local.push((p.clone(), text));
                }
            }
            for md in idx.files_of(Language::Markdown).iter().take(5) {
                if !local.iter().any(|(p, _)| p == md) {
                    if let Ok(text) = std::fs::read_to_string(md) {
                        local.push((md.clone(), text));
                    }
                }
            }
            if !local.is_empty() {
                docs_by_project.insert(proj.clone(), local);
            }
        }

        let project_names: Vec<String> = ctx.ecosystem.iter().map(|p| p.name.clone()).collect();
        let mut xrefs: HashMap<(String, String), Vec<(PathBuf, String)>> = HashMap::new();
        for (src, docs) in &docs_by_project {
            for (path, text) in docs {
                for dst in &project_names {
                    if dst == src {
                        continue;
                    }
                    let lower = text.to_ascii_lowercase();
                    let needle = dst.to_ascii_lowercase();
                    if let Some(pos) = find_word(&lower, &needle) {
                        let line_start = lower[..pos].rfind('\n').map(|n| n + 1).unwrap_or(0);
                        let line_end = lower[pos..]
                            .find('\n')
                            .map(|n| pos + n)
                            .unwrap_or(lower.len());
                        let snippet = text[line_start..line_end].trim().to_string();
                        xrefs
                            .entry((src.clone(), dst.clone()))
                            .or_default()
                            .push((path.clone(), snippet));
                    }
                }
            }
        }

        let mut out = Vec::new();
        for ((src, dst), mentions) in xrefs {
            let id = Synergy::stable_id("DocCrossRef", &src, &dst, "xref");
            let auto = auto_score_from(mentions.len(), 0.6, type_floor("DocCrossRef"));
            out.push(Synergy {
                id: id.clone(),
                synergy_type: "DocCrossRef".into(),
                source_project: src.clone(),
                target_project: dst.clone(),
                description: format!(
                    "Les docs de `{}` mentionnent `{}` à {} reprise(s)",
                    src,
                    dst,
                    mentions.len()
                ),
                priority: priority_from_score(auto),
                effort: "easy".into(),
                value: "low".into(),
                auto_score: auto,
                adjusted_score: auto,
                evidence: mentions
                    .iter()
                    .take(5)
                    .map(|(p, s)| Evidence {
                        project: src.clone(),
                        path: p.clone(),
                        line: None,
                        fragment: s.clone(),
                        weight: 0.6,
                    })
                    .collect(),
                suggested_action: Some(format!(
                    "Vérifier dans le code si `{}` dépend réellement de `{}` ; formaliser si oui",
                    src, dst
                )),
                fingerprint: blake3::hash(format!("xref:{}:{}", src, dst).as_bytes()).to_string(),
                ..Default::default()
            });
        }

        out.sort_by(|x, y| {
            y.auto_score
                .partial_cmp(&x.auto_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(40);
        Ok(out)
    }
}

fn find_word(hay: &str, needle: &str) -> Option<usize> {
    if needle.len() < 3 {
        return None;
    }
    let mut start = 0;
    while let Some(pos) = hay[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0
            || (!hay.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && hay.as_bytes()[abs - 1] != b'_');
        let after = abs + needle.len();
        let after_ok = after >= hay.len()
            || (!hay.as_bytes()[after].is_ascii_alphanumeric() && hay.as_bytes()[after] != b'_');
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + needle.len();
    }
    None
}
