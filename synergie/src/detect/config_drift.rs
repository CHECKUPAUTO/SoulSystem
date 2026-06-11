//! Détecteur config_drift : clés TOML/YAML/.env/JSON partagées ≥ 50 %.

use crate::analyzer::{auto_score_from, priority_from_score, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Default)]
pub struct ConfigDriftDetector;

impl Detector for ConfigDriftDetector {
    fn name(&self) -> &'static str {
        "config_drift"
    }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let mut configs: Vec<(String, PathBuf, HashSet<String>)> = Vec::new();
        for (proj, idx) in ctx.file_index.iter() {
            for p in &idx.configs {
                if let Some(keys) = extract_keys(p) {
                    if keys.len() >= 5 {
                        configs.push((proj.clone(), p.clone(), keys));
                    }
                }
            }
        }

        let mut out = Vec::new();
        for i in 0..configs.len() {
            for j in (i + 1)..configs.len() {
                if configs[i].0 == configs[j].0 {
                    continue;
                }
                let a = &configs[i].2;
                let b = &configs[j].2;
                let inter = a.intersection(b).count();
                if inter < 5 {
                    continue;
                }
                let union = a.len() + b.len() - inter;
                let jac = inter as f32 / union.max(1) as f32;
                if jac < 0.50 {
                    continue;
                }

                let shared: Vec<String> = a.intersection(b).take(10).cloned().collect();
                let id = Synergy::stable_id(
                    "ConfigUnification",
                    &configs[i].0,
                    &configs[j].0,
                    &format!(
                        "{}~{}",
                        configs[i]
                            .1
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(""),
                        configs[j]
                            .1
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                    ),
                );
                let auto = auto_score_from(2, jac, type_floor("ConfigUnification"));
                out.push(Synergy {
                    id: id.clone(),
                    synergy_type: "ConfigUnification".into(),
                    source_project: configs[i].0.clone(),
                    target_project: configs[j].0.clone(),
                    description: format!(
                        "Configs `{}` ↔ `{}` partagent {} clés (Jaccard {:.0}%) : {}",
                        configs[i].1.display(),
                        configs[j].1.display(),
                        inter,
                        jac * 100.0,
                        shared.join(", ")
                    ),
                    priority: priority_from_score(auto),
                    effort: "easy".into(),
                    value: "medium".into(),
                    auto_score: auto,
                    adjusted_score: auto,
                    evidence: vec![
                        Evidence {
                            project: configs[i].0.clone(),
                            path: configs[i].1.clone(),
                            line: None,
                            fragment: format!("{} clés communes", inter),
                            weight: jac,
                        },
                        Evidence {
                            project: configs[j].0.clone(),
                            path: configs[j].1.clone(),
                            line: None,
                            fragment: format!("{} clés communes", inter),
                            weight: jac,
                        },
                    ],
                    suggested_action: Some(
                        "Extraire un schéma commun (`commons-config`) chargé par les deux projets"
                            .to_string(),
                    ),
                    fingerprint: blake3::hash(format!("cfg:{}:{}", id, inter).as_bytes())
                        .to_string(),
                    ..Default::default()
                });
            }
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

fn extract_keys(path: &PathBuf) -> Option<HashSet<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let mut keys = HashSet::new();
    match ext.to_ascii_lowercase().as_str() {
        "toml" => collect_toml(&text, &mut keys),
        "yaml" | "yml" => collect_yaml(&text, &mut keys),
        "env" => collect_env(&text, &mut keys),
        "json" => collect_json(&text, &mut keys),
        _ => {
            if path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with(".env"))
                .unwrap_or(false)
            {
                collect_env(&text, &mut keys);
            }
        }
    }
    Some(keys)
}

fn collect_toml(text: &str, out: &mut HashSet<String>) {
    if let Ok(v) = toml::from_str::<toml::Value>(text) {
        walk_toml(&v, "", out);
    }
}

fn walk_toml(v: &toml::Value, prefix: &str, out: &mut HashSet<String>) {
    if let toml::Value::Table(t) = v {
        for (k, vv) in t {
            let np = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{}.{}", prefix, k)
            };
            walk_toml(vv, &np, out);
            out.insert(np);
        }
    }
}

fn collect_json(text: &str, out: &mut HashSet<String>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        walk_json(&v, "", out);
    }
}

fn walk_json(v: &serde_json::Value, prefix: &str, out: &mut HashSet<String>) {
    if let serde_json::Value::Object(m) = v {
        for (k, vv) in m {
            let np = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{}.{}", prefix, k)
            };
            walk_json(vv, &np, out);
            out.insert(np);
        }
    }
}

fn collect_env(text: &str, out: &mut HashSet<String>) {
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(eq) = l.find('=') {
            let k = l[..eq].trim().to_string();
            if !k.is_empty() {
                out.insert(k);
            }
        }
    }
}

fn collect_yaml(text: &str, out: &mut HashSet<String>) {
    let mut stack: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if let Some(colon) = trimmed.find(':') {
            let key = trimmed[..colon].trim().to_string();
            if key.starts_with('-') || key.is_empty() {
                continue;
            }
            while let Some((i, _)) = stack.last() {
                if *i >= indent {
                    stack.pop();
                } else {
                    break;
                }
            }
            let dotted = if stack.is_empty() {
                key.clone()
            } else {
                let prefix: Vec<&str> = stack.iter().map(|(_, k)| k.as_str()).collect();
                format!("{}.{}", prefix.join("."), key)
            };
            out.insert(dotted);
            let after = trimmed[colon + 1..].trim();
            if after.is_empty() {
                stack.push((indent, key));
            }
        }
    }
}
