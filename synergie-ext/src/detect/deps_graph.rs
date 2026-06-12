//! Détecteur deps_graph : dépendances partagées (paire-à-paire) + Fusion
//! quand l'overlap dépasse 40 % (Jaccard ≥ 0.30).

use crate::analyzer::{auto_score_from, priority_from_score, short_hash, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct DepsGraphDetector;

static NOISE: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        "serde", "serde_json", "tokio", "anyhow", "thiserror", "tracing",
        "log", "env_logger", "chrono", "uuid", "once_cell", "futures",
        "async-trait", "regex", "bytes", "hex", "base64", "rand",
        "requests", "urllib3", "click", "rich", "pydantic", "pyyaml",
        "numpy", "pandas", "tqdm", "loguru",
        "lodash", "chalk", "axios", "express", "dotenv",
        "github.com/stretchr/testify",
    ])
});

impl Detector for DepsGraphDetector {
    fn name(&self) -> &'static str { "deps_graph" }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let mut by_dep: HashMap<String, Vec<String>> = HashMap::new();
        let mut by_project: HashMap<String, HashSet<String>> = HashMap::new();

        for (proj, deps) in ctx.deps.iter() {
            let mut set = HashSet::new();
            for d in &deps.dependencies {
                let n = d.name.to_lowercase();
                if n.is_empty() { continue; }
                by_dep.entry(n.clone()).or_default().push(proj.clone());
                set.insert(n);
            }
            by_project.insert(proj.clone(), set);
        }

        let mut out: Vec<Synergy> = Vec::new();

        // A) SharedDependency par paire (a, b) pour CHAQUE dépendance non-bruyante partagée.
        for (dep, projs) in by_dep.iter() {
            if projs.len() < 2 { continue; }
            if NOISE.contains(dep.as_str()) && projs.len() < 4 { continue; }
            // dedup + sort
            let mut uniq: Vec<&String> = projs.iter().collect();
            uniq.sort();
            uniq.dedup();
            for i in 0..uniq.len() {
                for j in (i + 1)..uniq.len() {
                    let a = uniq[i].as_str();
                    let b = uniq[j].as_str();
                    let target = format!("commons-{}", dep);
                    let id = Synergy::stable_id("SharedDependency", a, b, &target);
                    let auto = auto_score_from(2, 0.85, type_floor("SharedDependency"));
                    out.push(Synergy {
                        id: id.clone(),
                        synergy_type: "SharedDependency".into(),
                        source_project: a.to_string(),
                        target_project: b.to_string(),
                        description: format!(
                            "Les projets `{a}` et `{b}` partagent la dépendance `{dep}` — candidat à une crate commune.",
                            a = a, b = b, dep = dep
                        ),
                        priority: priority_from_score(auto),
                        effort: "easy".into(),
                        value: if uniq.len() >= 4 { "high".into() } else { "medium".into() },
                        auto_score: auto,
                        adjusted_score: auto,
                        evidence: vec![
                            evidence_for_dep(ctx, a, dep),
                            evidence_for_dep(ctx, b, dep),
                        ],
                        suggested_action: Some(format!(
                            "Créer `commons-{dep}` (façade/wrapper) et migrer {a} + {b}",
                            dep = dep, a = a, b = b
                        )),
                        fingerprint: blake3::hash(format!("dep:{}:{}:{}", dep, a, b).as_bytes()).to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        // B) Fusion si overlap > 40% (Jaccard ≥ 0.30) entre deux projets.
        let pnames: Vec<&String> = by_project.keys().collect();
        for i in 0..pnames.len() {
            for j in (i + 1)..pnames.len() {
                let a = pnames[i];
                let b = pnames[j];
                let da = &by_project[a];
                let db = &by_project[b];
                if da.len() < 3 || db.len() < 3 { continue; }
                let inter = da.intersection(db).count();
                let smaller = da.len().min(db.len());
                let ratio = inter as f32 / smaller as f32;
                if ratio < 0.40 { continue; }
                let union = da.union(db).count();
                let jac = inter as f32 / union.max(1) as f32;
                if jac < 0.30 { continue; }
                let shared: Vec<String> = da.intersection(db).take(8).cloned().collect();
                let id = Synergy::stable_id("Fusion", a, b, "fusion");
                let auto = auto_score_from(2, jac, type_floor("Fusion"));
                out.push(Synergy {
                    id: id.clone(),
                    synergy_type: "Fusion".into(),
                    source_project: a.clone(),
                    target_project: b.clone(),
                    description: format!(
                        "Fusion potentielle (Jaccard {:.0}%): {} ↔ {} — {} deps communes",
                        jac * 100.0, a, b, inter
                    ),
                    priority: priority_from_score(auto),
                    effort: "hard".into(),
                    value: "high".into(),
                    auto_score: auto,
                    adjusted_score: auto,
                    evidence: shared.iter().map(|d| Evidence {
                        project: format!("{}↔{}", a, b),
                        path: std::path::PathBuf::from("(deps)"),
                        line: None,
                        fragment: d.clone(),
                        weight: jac,
                    }).collect(),
                    suggested_action: Some(format!(
                        "Audit `{a}` ↔ `{b}` : (A) fusion en `{a}-{b}` OU (B) `commons-{a}-{b}` + APIs disjointes",
                        a = a, b = b
                    )),
                    fingerprint: blake3::hash(format!("fusion:{}:{}", a, b).as_bytes()).to_string(),
                    ..Default::default()
                });
                let _ = short_hash(&id);
            }
        }

        out.sort_by(|x, y| y.auto_score.partial_cmp(&x.auto_score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(120);
        Ok(out)
    }
}

fn evidence_for_dep(ctx: &DetectContext, project: &str, dep: &str) -> Evidence {
    let mut frag = format!("{}", dep);
    let mut path = std::path::PathBuf::from("(deps)");
    if let Some(pd) = ctx.deps.get(project) {
        if let Some(d) = pd.dependencies.iter().find(|d| d.name.to_lowercase() == dep) {
            path = d.manifest.clone();
            frag = format!("{} {}", d.name, d.version.as_deref().unwrap_or("*"));
        }
    }
    Evidence {
        project: project.to_string(),
        path,
        line: None,
        fragment: frag,
        weight: 0.85,
    }
}
