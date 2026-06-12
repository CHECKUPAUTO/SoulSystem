//! Détecteur cocommit : utilise la `CoCommitMatrix` pour repérer
//! les couplages cachés entre projets.

use crate::analyzer::{auto_score_from, priority_from_score, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct CoCommitDetector;

impl Detector for CoCommitDetector {
    fn name(&self) -> &'static str { "cocommit" }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let Some(matrix) = ctx.cocommits.as_ref() else {
            return Ok(vec![]);
        };
        let pairs = matrix.top_pairs(0.55, 200);
        if pairs.is_empty() {
            return Ok(vec![]);
        }

        let mut path_to_project: HashMap<PathBuf, String> = HashMap::new();
        for p in ctx.ecosystem.iter() {
            let canon = p.path.canonicalize().ok().unwrap_or_else(|| p.path.clone());
            path_to_project.insert(canon, p.name.clone());
        }
        let find_owner = |path: &str| -> Option<String> {
            let pb = PathBuf::from(path);
            for (root, name) in &path_to_project {
                if pb.starts_with(root) || path.contains(name) {
                    return Some(name.clone());
                }
            }
            None
        };

        // (pa, pb) → liste de paires de fichiers avec jaccard.
        let mut inter: HashMap<(String, String), Vec<(String, String, f32)>> = HashMap::new();
        for (a, b, jac) in pairs {
            let (Some(pa), Some(pb)) = (find_owner(&a), find_owner(&b)) else { continue };
            if pa == pb { continue; }
            let key = if pa < pb { (pa.clone(), pb.clone()) } else { (pb.clone(), pa.clone()) };
            inter.entry(key).or_default().push((a, b, jac));
        }

        let mut out = Vec::new();
        for ((pa, pb), entries) in inter {
            let mean_jac = entries.iter().map(|(_, _, j)| *j).sum::<f32>() / entries.len() as f32;
            let id = Synergy::stable_id("HiddenCoupling", &pa, &pb, "cocommit");
            let auto = auto_score_from(entries.len(), mean_jac, type_floor("HiddenCoupling"));
            out.push(Synergy {
                id: id.clone(),
                synergy_type: "HiddenCoupling".into(),
                source_project: pa.clone(),
                target_project: pb.clone(),
                description: format!(
                    "Couplage caché : {} ↔ {} ({} paires de fichiers co-commitées, Jaccard moyen {:.2})",
                    pa, pb, entries.len(), mean_jac
                ),
                priority: priority_from_score(auto + 0.2),
                effort: "easy".into(),
                value: if entries.len() >= 5 { "high".into() } else { "medium".into() },
                auto_score: auto,
                adjusted_score: auto,
                evidence: entries.iter().take(6).flat_map(|(a, b, j)| {
                    vec![
                        Evidence { project: pa.clone(), path: PathBuf::from(a), line: None, fragment: format!("co-committed with {} (J={:.2})", b, j), weight: *j },
                        Evidence { project: pb.clone(), path: PathBuf::from(b), line: None, fragment: format!("co-committed with {} (J={:.2})", a, j), weight: *j },
                    ]
                }).collect(),
                suggested_action: Some(format!(
                    "Formaliser le contrat implicite entre `{a}` et `{b}` dans une crate `iface-{a}-{b}` + test d'intégration",
                    a = pa, b = pb
                )),
                fingerprint: blake3::hash(format!("cc:{}:{}:{:.2}", pa, pb, mean_jac).as_bytes()).to_string(),
                ..Default::default()
            });
        }

        out.sort_by(|x, y| y.auto_score.partial_cmp(&x.auto_score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(30);
        Ok(out)
    }
}
