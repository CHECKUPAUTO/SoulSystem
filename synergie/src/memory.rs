//! Persistance complète SYNERGIE.
//!
//! Trees sled :
//!   - `meta`              → état R-STDP, métadonnées globales
//!   - `synergies`         → id → Synergy JSON
//!   - `embeddings_cache`  → blake3(text) → Vec<f32> (bincode)
//!   - `scores`            → id → f32 (bincode)
//!   - `history`           → "report:<rfc3339>" → ScanReport JSON
//!   - `feedback`          → uuid → FeedbackEntry JSON

use crate::rstdp::RSTDPOptimizer;
use crate::types::{ScanReport, Synergy, SynergyStatus};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Default path for the sled database.
const DEFAULT_DB_PATH: &str = "/mnt/nvme/soullink_brain/synergies/synergie.sled";
const RSTDP_KEY: &[u8] = b"rstdp_state";

fn db_path() -> PathBuf {
    std::env::var("SYNERGIE_DB")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DB_PATH))
}

fn db() -> &'static sled::Db {
    static DB: OnceLock<sled::Db> = OnceLock::new();
    DB.get_or_init(|| {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        sled::open(&path).expect("sled DB open")
    })
}

// ─── R-STDP (existant) ──────────────────────────────────────────────────

pub fn load_optimizer() -> Result<RSTDPOptimizer> {
    let tree = db().open_tree("meta")?;
    match tree.get(RSTDP_KEY)? {
        Some(bytes) => {
            let opt: RSTDPOptimizer =
                bincode::deserialize(&bytes).context("deserialize RSTDPOptimizer")?;
            Ok(opt)
        }
        None => Ok(RSTDPOptimizer::new(0.1, 0.9)),
    }
}

pub fn save_optimizer(opt: &RSTDPOptimizer) -> Result<()> {
    let tree = db().open_tree("meta")?;
    let bytes = bincode::serialize(opt).context("serialize RSTDPOptimizer")?;
    tree.insert(RSTDP_KEY, bytes)?;
    tree.flush()?;
    Ok(())
}

// ─── Synergies (nouveau) ────────────────────────────────────────────────

pub fn upsert_synergy(s: &Synergy) -> Result<()> {
    let tree = db().open_tree("synergies")?;
    let bytes = serde_json::to_vec(s)?;
    tree.insert(s.id.as_bytes(), bytes)?;
    Ok(())
}

pub fn get_synergy(id: &str) -> Result<Option<Synergy>> {
    let tree = db().open_tree("synergies")?;
    match tree.get(id.as_bytes())? {
        Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        None => Ok(None),
    }
}

pub fn list_synergies() -> Result<Vec<Synergy>> {
    let tree = db().open_tree("synergies")?;
    let mut out = Vec::new();
    for r in tree.iter() {
        let (_, v) = r?;
        if let Ok(s) = serde_json::from_slice::<Synergy>(&v) {
            out.push(s);
        }
    }
    Ok(out)
}

pub fn delete_synergy(id: &str) -> Result<bool> {
    let tree = db().open_tree("synergies")?;
    Ok(tree.remove(id.as_bytes())?.is_some())
}

/// Marque comme `Stale` les synergies non vues dans `seen_ids` et dont la
/// dernière vue date d'avant `stale_after_days`.
pub fn mark_unseen_stale(
    seen: &std::collections::HashSet<String>,
    stale_after_days: i64,
) -> Result<usize> {
    let all = list_synergies()?;
    let mut count = 0;
    let threshold = stale_after_days.max(1);
    for mut s in all {
        if seen.contains(&s.id) {
            continue;
        }
        if matches!(
            s.status,
            SynergyStatus::Implemented | SynergyStatus::Rejected | SynergyStatus::Stale
        ) {
            continue;
        }
        let elapsed = chrono::Utc::now().signed_duration_since(s.last_seen);
        if elapsed.num_days() >= threshold {
            s.status = SynergyStatus::Stale;
            upsert_synergy(&s)?;
            count += 1;
        }
    }
    Ok(count)
}

// ─── Embeddings cache (nouveau) ─────────────────────────────────────────

pub fn embedding_cache() -> Result<Arc<sled::Tree>> {
    Ok(Arc::new(db().open_tree("embeddings_cache")?))
}

// ─── History / Reports (nouveau) ────────────────────────────────────────

pub fn save_report(report: &ScanReport, reports_dir: &Path) -> Result<PathBuf> {
    // Sled history.
    let tree = db().open_tree("history")?;
    let key = format!("report:{}", report.generated_at.to_rfc3339());
    let bytes = serde_json::to_vec(report)?;
    tree.insert(key.as_bytes(), bytes.clone())?;
    tree.flush()?;

    // Latest pointer.
    let meta = db().open_tree("meta")?;
    meta.insert(b"last_report_key", key.as_bytes())?;
    meta.flush()?;

    // Disk artefacts (JSON + MD + latest.json).
    std::fs::create_dir_all(reports_dir).ok();
    let stamp = report.generated_at.format("%Y%m%dT%H%M%SZ").to_string();
    let json_path = reports_dir.join(format!("report-{}.json", stamp));
    let md_path = reports_dir.join(format!("report-{}.md", stamp));
    let latest_json = reports_dir.join("latest.json");
    std::fs::write(&json_path, &bytes)?;
    std::fs::write(&latest_json, &bytes)?;
    std::fs::write(&md_path, render_markdown(report))?;
    Ok(json_path)
}

pub fn last_report() -> Result<Option<ScanReport>> {
    let meta = db().open_tree("meta")?;
    let history = db().open_tree("history")?;
    if let Some(key) = meta.get(b"last_report_key")? {
        if let Some(bytes) = history.get(&key)? {
            return Ok(Some(serde_json::from_slice(&bytes)?));
        }
    }
    // Fallback : dernière entrée par ordre lexico (timestamps RFC-3339 triables).
    let mut last: Option<ScanReport> = None;
    for r in history.iter() {
        let (_, v) = r?;
        if let Ok(rep) = serde_json::from_slice::<ScanReport>(&v) {
            last = Some(rep);
        }
    }
    Ok(last)
}

pub fn prune_history(max_keep: usize) -> Result<()> {
    let history = db().open_tree("history")?;
    let mut keys: Vec<sled::IVec> = history.iter().keys().filter_map(|k| k.ok()).collect();
    if keys.len() <= max_keep {
        return Ok(());
    }
    keys.sort();
    let to_drop = keys.len() - max_keep;
    for k in keys.into_iter().take(to_drop) {
        history.remove(k)?;
    }
    Ok(())
}

// ─── Feedback (nouveau) ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiFeedbackEntry {
    pub synergy_id: String,
    pub status: SynergyStatus,
    pub note: Option<String>,
    pub at: chrono::DateTime<chrono::Utc>,
}

/// Enregistre un feedback UI/MCP : met à jour le statut + applique
/// la récompense R-STDP sur le coefficient du type de la synergie.
pub fn record_feedback(
    synergy_id: &str,
    status: SynergyStatus,
    note: Option<String>,
) -> Result<Synergy> {
    let mut s = get_synergy(synergy_id)?
        .ok_or_else(|| anyhow::anyhow!("synergy not found: {synergy_id}"))?;
    s.status = status;
    upsert_synergy(&s)?;

    // R-STDP.
    let mut opt = load_optimizer()?;
    let reward: f64 = match status {
        SynergyStatus::Implemented => 0.10,
        SynergyStatus::Rejected => -0.15,
        _ => 0.0,
    };
    if reward.abs() > 1e-9 {
        opt.reinforce(s.synergy_type_label(), reward, note.as_deref());
        save_optimizer(&opt)?;
    }

    // Trace.
    let tree = db().open_tree("feedback")?;
    let entry = UiFeedbackEntry {
        synergy_id: s.id.clone(),
        status,
        note,
        at: chrono::Utc::now(),
    };
    let uuid = uuid::Uuid::new_v4().to_string();
    tree.insert(uuid.as_bytes(), serde_json::to_vec(&entry)?)?;
    Ok(s)
}

pub fn list_feedback() -> Result<Vec<UiFeedbackEntry>> {
    let tree = db().open_tree("feedback")?;
    let mut out = Vec::new();
    for r in tree.iter() {
        let (_, v) = r?;
        if let Ok(e) = serde_json::from_slice::<UiFeedbackEntry>(&v) {
            out.push(e);
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.at));
    Ok(out)
}

// ─── Counts ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct MemoryCounts {
    pub synergies: usize,
    pub embed_cache: usize,
    pub history: usize,
    pub feedback: usize,
}

pub fn counts() -> Result<MemoryCounts> {
    Ok(MemoryCounts {
        synergies: db().open_tree("synergies")?.len(),
        embed_cache: db().open_tree("embeddings_cache")?.len(),
        history: db().open_tree("history")?.len(),
        feedback: db().open_tree("feedback")?.len(),
    })
}

// ─── Markdown render ────────────────────────────────────────────────────

fn render_markdown(r: &ScanReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# SYNERGIE — Rapport {}\n\n",
        r.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    s.push_str(&format!(
        "- **Projets scannés** : {}\n- **Synergies détectées** : {}\n- **Durée** : {} ms\n\n",
        r.projects.len(),
        r.synergies.len(),
        r.elapsed_ms
    ));

    s.push_str("## Statistiques\n\n");
    let mut by_type: Vec<(&String, &usize)> = r.stats.by_type.iter().collect();
    by_type.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in &by_type {
        s.push_str(&format!("- `{}` : {}\n", k, v));
    }
    s.push_str(&format!(
        "\n_Score ajusté moyen :_ **{:.2}**\n\n",
        r.stats.mean_adjusted_score
    ));

    s.push_str("## Top 20 synergies (par score ajusté)\n\n");
    let mut sorted = r.synergies.clone();
    sorted.sort_by(|a, b| {
        b.adjusted_score
            .partial_cmp(&a.adjusted_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for syn in sorted.iter().take(20) {
        s.push_str(&format!(
            "### [{score:.2}] {title}\n\n- **type** : `{ty}` | **prio** : {pr} | **effort** : {ef} | **valeur** : {val} | **status** : {st:?}\n- **id** : `{id}`\n- **{src}** → **{tgt}**\n\n{desc}\n\n",
            score = syn.adjusted_score,
            title = first_line(&syn.description),
            ty = syn.synergy_type,
            pr = syn.priority,
            ef = syn.effort,
            val = syn.value,
            st = syn.status,
            id = syn.id,
            src = syn.source_project,
            tgt = syn.target_project,
            desc = syn.description
        ));
        if let Some(act) = &syn.suggested_action {
            s.push_str(&format!("**Action suggérée :**\n```\n{}\n```\n\n", act));
        }
        if !syn.evidence.is_empty() {
            s.push_str("**Evidence :**\n\n");
            for e in syn.evidence.iter().take(5) {
                s.push_str(&format!(
                    "- `{}:{}` — {}\n",
                    e.path.display(),
                    e.line.map(|x| x.to_string()).unwrap_or_else(|| "-".into()),
                    e.fragment.lines().next().unwrap_or("")
                ));
            }
            s.push('\n');
        }
    }
    s
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

// ─── Tests existants conservés ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "OnceLock fige la DB pour tout le process — lancer en isolé"]
    fn test_load_optimizer_default() {
        // Test isolé : un DB temporaire.
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("SYNERGIE_DB", tmp.path().join("test.sled"));
        let opt = load_optimizer().expect("load default");
        assert!((opt.lr() - 0.1).abs() < 1e-9);
        assert!((opt.momentum() - 0.9).abs() < 1e-9);
    }
}
