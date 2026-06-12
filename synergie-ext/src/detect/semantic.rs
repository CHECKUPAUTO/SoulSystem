//! Détecteur sémantique : embeddings (local/HTTP) + cosine ≥ threshold.

use crate::analyzer::{auto_score_from, priority_from_score, short_hash, type_floor};
use crate::detect::DetectContext;
use crate::embed::{cosine, EmbedClient};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::HashMap;
use tracing::{info, warn};

const MIN_SIG_LEN: usize = 30;
const MAX_EMBED_TEXTS: usize = 1500;

struct Entry {
    project: String,
    item_idx: usize,
    text: String,
}

pub async fn run(ctx: &DetectContext, embed: &mut EmbedClient) -> Result<Vec<Synergy>> {
    let mut entries: Vec<Entry> = Vec::new();
    for (proj, items) in ctx.public_items.iter() {
        for (idx, it) in items.iter().enumerate() {
            if !it.kind.is_callable() { continue; }
            let mut text = String::new();
            if let Some(d) = &it.doc {
                text.push_str(d.trim());
                text.push('\n');
            }
            text.push_str(&it.signature);
            if text.trim().len() < MIN_SIG_LEN { continue; }
            entries.push(Entry { project: proj.clone(), item_idx: idx, text });
        }
    }
    if entries.len() > MAX_EMBED_TEXTS {
        entries.sort_by_key(|e| std::cmp::Reverse(e.text.len()));
        entries.truncate(MAX_EMBED_TEXTS);
    }
    if entries.is_empty() {
        return Ok(vec![]);
    }
    info!(target: "detect.semantic", n = entries.len(), "embedding items");

    let texts: Vec<String> = entries.iter().map(|e| e.text.clone()).collect();
    let vectors = match embed.embed_batch(&texts).await {
        Ok(v) => v,
        Err(e) => {
            warn!(target: "detect.semantic", error = %e, "embeddings indisponibles");
            return Ok(vec![]);
        }
    };

    let threshold = ctx.cfg.semantic_threshold.clamp(0.5, 0.99);

    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        buckets.entry(e.text.len() / 60).or_default().push(i);
    }

    let mut out = Vec::new();
    let bkeys: Vec<usize> = buckets.keys().cloned().collect();
    for &bk in &bkeys {
        for off in [0isize, 1, -1] {
            let target = bk as isize + off;
            if target < 0 { continue; }
            let tk = target as usize;
            if !buckets.contains_key(&tk) { continue; }
            let a = &buckets[&bk];
            let b = &buckets[&tk];
            for &i in a {
                for &j in b {
                    if i >= j { continue; }
                    if entries[i].project == entries[j].project { continue; }
                    let sim = cosine(&vectors[i], &vectors[j]);
                    if sim < threshold { continue; }
                    if let Some(syn) = build_synergy(ctx, &entries[i], &entries[j], sim) {
                        out.push(syn);
                    }
                }
            }
        }
    }

    out.sort_by(|x, y| y.auto_score.partial_cmp(&x.auto_score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(40);
    Ok(out)
}

fn build_synergy(ctx: &DetectContext, a: &Entry, b: &Entry, sim: f32) -> Option<Synergy> {
    let item_a = ctx.public_items.get(&a.project).and_then(|v| v.get(a.item_idx))?;
    let item_b = ctx.public_items.get(&b.project).and_then(|v| v.get(b.item_idx))?;
    let auto = auto_score_from(2, sim, type_floor("SemanticOverlap"));
    let priority = priority_from_score(auto);
    let id = Synergy::stable_id(
        "SemanticOverlap",
        &a.project, &b.project,
        &format!("{}~{}", item_a.name, item_b.name),
    );
    let description = format!(
        "Recouvrement sémantique (cos {:.3}) : `{}::{}` ↔ `{}::{}`",
        sim, a.project, item_a.name, b.project, item_b.name
    );
    Some(Synergy {
        id: id.clone(),
        synergy_type: "SemanticOverlap".into(),
        source_project: a.project.clone(),
        target_project: b.project.clone(),
        description,
        priority,
        effort: "medium".into(),
        value: if sim > 0.94 { "high".into() } else { "medium".into() },
        auto_score: auto,
        adjusted_score: auto,
        evidence: vec![
            Evidence {
                project: a.project.clone(),
                path: item_a.file.clone(),
                line: Some(item_a.line),
                fragment: format!("{} {}", item_a.kind_label(), item_a.name),
                weight: sim,
            },
            Evidence {
                project: b.project.clone(),
                path: item_b.file.clone(),
                line: Some(item_b.line),
                fragment: format!("{} {}", item_b.kind_label(), item_b.name),
                weight: sim,
            },
        ],
        suggested_action: Some(format!(
            "Comparer les implémentations et fusionner dans une crate `commons-{}`",
            short_hash(&id)
        )),
        fingerprint: blake3::hash(format!("sem:{}:{:.3}", id, sim).as_bytes()).to_string(),
        ..Default::default()
    })
}
