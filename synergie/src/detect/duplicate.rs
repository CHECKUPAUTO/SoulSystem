//! Détecteur duplicate : MinHash 128 + LSH banding + Jaccard exact.
//!
//! Émet une synergie par PAIRE (source_project, target_project) — donc
//! plusieurs synergies si la même fonction apparaît dans 3 projets.

use crate::analyzer::{auto_score_from, priority_from_score, short_hash, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::scanner::{ItemKind, PublicItem};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use twox_hash::XxHash64;

const MIN_TOKENS: usize = 12;

#[derive(Default)]
pub struct DuplicateDetector;

impl Detector for DuplicateDetector {
    fn name(&self) -> &'static str {
        "duplicate"
    }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let n_sig = ctx.cfg.minhash_signatures.max(32);
        let threshold = ctx.cfg.duplicate_jaccard_min.clamp(0.5, 0.99);

        struct Slot<'a> {
            project: &'a str,
            item: &'a PublicItem,
            tokens: HashSet<u64>,
            signature: Vec<u64>,
        }
        let mut slots: Vec<Slot> = Vec::new();
        for (proj, items) in ctx.public_items.iter() {
            for it in items {
                if !it.kind.is_callable() {
                    continue;
                }
                if it.body_tokens.len() < MIN_TOKENS {
                    continue;
                }
                let token_hashes: HashSet<u64> = it
                    .body_tokens
                    .iter()
                    .map(|t| {
                        let mut h = XxHash64::with_seed(0xA5A5A5A5);
                        h.write(t.as_bytes());
                        h.finish()
                    })
                    .collect();
                if token_hashes.len() < MIN_TOKENS {
                    continue;
                }
                let signature = minhash_signature(&token_hashes, n_sig);
                slots.push(Slot {
                    project: proj.as_str(),
                    item: it,
                    tokens: token_hashes,
                    signature,
                });
            }
        }

        let r = lsh_r_for(threshold, n_sig);
        let b = (n_sig / r).max(1);
        let mut buckets: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
        for (idx, s) in slots.iter().enumerate() {
            for band in 0..b {
                let start = band * r;
                let end = (start + r).min(s.signature.len());
                if end <= start {
                    break;
                }
                let mut h = XxHash64::with_seed(0xDEADBEEF + band as u64);
                for &x in &s.signature[start..end] {
                    h.write(&x.to_le_bytes());
                }
                buckets.entry((band, h.finish())).or_default().push(idx);
            }
        }

        let mut candidate_pairs: HashSet<(usize, usize)> = HashSet::new();
        for (_, ids) in buckets.iter() {
            if ids.len() < 2 {
                continue;
            }
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let (a, b) = (ids[i], ids[j]);
                    if slots[a].project == slots[b].project {
                        continue;
                    }
                    let key = if a < b { (a, b) } else { (b, a) };
                    candidate_pairs.insert(key);
                }
            }
        }

        let mut out = Vec::new();
        for (i, j) in candidate_pairs {
            let jac = jaccard(&slots[i].tokens, &slots[j].tokens);
            if jac < threshold {
                continue;
            }
            let (a, b) = (&slots[i], &slots[j]);
            let auto = auto_score_from(2, jac, type_floor("Deduplication"));
            let priority = priority_from_score(auto);
            let id = Synergy::stable_id(
                "Deduplication",
                a.project,
                b.project,
                &format!("{}~{}", a.item.name, b.item.name),
            );
            let description = format!(
                "Fonctions quasi-identiques (Jaccard {:.2}) : `{}::{}` ↔ `{}::{}`",
                jac, a.project, a.item.name, b.project, b.item.name
            );
            let evidence = vec![
                Evidence {
                    project: a.project.to_string(),
                    path: a.item.file.clone(),
                    line: Some(a.item.line),
                    fragment: format!("{} {}", a.item.kind_label(), a.item.name),
                    weight: jac,
                },
                Evidence {
                    project: b.project.to_string(),
                    path: b.item.file.clone(),
                    line: Some(b.item.line),
                    fragment: format!("{} {}", b.item.kind_label(), b.item.name),
                    weight: jac,
                },
            ];

            out.push(Synergy {
                id: id.clone(),
                synergy_type: "Deduplication".into(),
                source_project: a.project.to_string(),
                target_project: b.project.to_string(),
                description,
                priority,
                effort: "medium".into(),
                value: if jac > 0.9 { "high".into() } else { "medium".into() },
                auto_score: auto,
                adjusted_score: auto,
                evidence,
                suggested_action: Some(format!(
                    "1. Extraire dans une crate `commons-{}`\n2. Migrer `{}::{}` et `{}::{}`\n3. PR taggée `synergie/dedup/{}`",
                    short_hash(&id),
                    a.project, a.item.name,
                    b.project, b.item.name,
                    id
                )),
                fingerprint: blake3::hash(format!("dup:{}:{}", id, jac).as_bytes()).to_string(),
                ..Default::default()
            });
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

fn minhash_signature(tokens: &HashSet<u64>, n_sig: usize) -> Vec<u64> {
    if tokens.is_empty() {
        return vec![u64::MAX; n_sig];
    }
    let mut sig = vec![u64::MAX; n_sig];
    for &t in tokens {
        for k in 0..n_sig {
            let mut h = XxHash64::with_seed(
                (0x9E3779B97F4A7C15u64).wrapping_mul((k as u64).wrapping_add(1)),
            );
            h.write(&t.to_le_bytes());
            let v = h.finish();
            if v < sig[k] {
                sig[k] = v;
            }
        }
    }
    sig
}

fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}

fn lsh_r_for(threshold: f32, n_sig: usize) -> usize {
    let mut best_r = 4;
    let mut best_dist = f32::MAX;
    for r in 2..=16 {
        if n_sig % r != 0 {
            continue;
        }
        let bb = n_sig / r;
        let approx = (1.0 / bb as f32).powf(1.0 / r as f32);
        let d = (approx - threshold).abs();
        if d < best_dist {
            best_dist = d;
            best_r = r;
        }
    }
    best_r
}

// ItemKind import marker
#[allow(dead_code)]
fn _ensure_itemkind_used(_k: ItemKind) {}
