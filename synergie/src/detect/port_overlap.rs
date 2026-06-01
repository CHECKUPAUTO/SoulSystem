//! Détecteur port_overlap : ports identiques (conflit) ou voisins (bus commun).

use crate::analyzer::{auto_score_from, priority_from_score, type_floor};
use crate::detect::{DetectContext, Detector};
use crate::types::{Evidence, Synergy};
use anyhow::Result;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Default)]
pub struct PortOverlapDetector;

#[derive(Debug, Clone)]
struct PortInfo {
    project: String,
    port: u16,
    source: String,
    path: PathBuf,
}

impl Detector for PortOverlapDetector {
    fn name(&self) -> &'static str {
        "port_overlap"
    }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let mut ports: Vec<PortInfo> = Vec::new();
        for p in ctx.ecosystem.iter() {
            for s in &p.services {
                if let Some(port) = s.port {
                    ports.push(PortInfo {
                        project: p.name.clone(),
                        port,
                        source: format!("{} ({})", s.source, s.kind),
                        path: p
                            .manifests
                            .first()
                            .cloned()
                            .unwrap_or_else(|| p.path.clone()),
                    });
                }
            }
        }
        if let Some(snap) = ctx.runtime.as_ref() {
            for l in &snap.listeners {
                let owner = l
                    .cwd
                    .as_ref()
                    .and_then(|cwd| project_owning(ctx, cwd))
                    .or_else(|| {
                        l.process
                            .as_ref()
                            .and_then(|exe| project_owning(ctx, &PathBuf::from(exe)))
                    });
                if let Some(name) = owner {
                    ports.push(PortInfo {
                        project: name,
                        port: l.port,
                        source: format!(
                            "runtime:{} pid={}",
                            l.process.as_deref().unwrap_or("?"),
                            l.pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
                        ),
                        path: PathBuf::from(format!("ss://{}:{}", l.proto, l.port)),
                    });
                }
            }
        }
        if ports.is_empty() {
            return Ok(vec![]);
        }

        let mut out: Vec<Synergy> = Vec::new();

        // A) Mêmes ports → ServiceBus haute priorité, paire-à-paire.
        let mut by_port: HashMap<u16, Vec<&PortInfo>> = HashMap::new();
        for p in &ports {
            by_port.entry(p.port).or_default().push(p);
        }
        for (port, infos) in &by_port {
            if infos.len() < 2 {
                continue;
            }
            for i in 0..infos.len() {
                for j in (i + 1)..infos.len() {
                    let a = infos[i];
                    let b = infos[j];
                    if a.project == b.project {
                        continue;
                    }
                    let id = Synergy::stable_id(
                        "ServiceBus",
                        &a.project,
                        &b.project,
                        &format!("port-{}", port),
                    );
                    let auto = auto_score_from(2, 1.0, type_floor("ServiceBus"));
                    out.push(Synergy {
                        id: id.clone(),
                        synergy_type: "ServiceBus".into(),
                        source_project: a.project.clone(),
                        target_project: b.project.clone(),
                        description: format!(
                            "Conflit de port {} entre `{}` et `{}` ({} ↔ {})",
                            port, a.project, b.project, a.source, b.source
                        ),
                        priority: priority_from_score(auto + 0.3),
                        effort: "easy".into(),
                        value: "high".into(),
                        auto_score: auto,
                        adjusted_score: auto,
                        evidence: vec![
                            Evidence { project: a.project.clone(), path: a.path.clone(), line: None, fragment: format!("port {} via {}", a.port, a.source), weight: 1.0 },
                            Evidence { project: b.project.clone(), path: b.path.clone(), line: None, fragment: format!("port {} via {}", b.port, b.source), weight: 1.0 },
                        ],
                        suggested_action: Some(format!(
                            "1. Réattribuer un port unique\n2. Si interaction prévue : bus (NATS/MQTT) commun\n3. Centraliser dans `/etc/synergie/ports.toml`",
                        )),
                        fingerprint: blake3::hash(format!("portconflict:{}:{}", a.project, b.project).as_bytes()).to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        // B) Ports voisins (≤ 16) → bus commun envisageable.
        let mut sorted_ports: BTreeMap<u16, Vec<&PortInfo>> = BTreeMap::new();
        for p in &ports {
            sorted_ports.entry(p.port).or_default().push(p);
        }
        let plist: Vec<u16> = sorted_ports.keys().cloned().collect();
        let mut groups: Vec<Vec<u16>> = Vec::new();
        let mut current: Vec<u16> = Vec::new();
        for &p in &plist {
            if current.is_empty() || p - current.last().unwrap() <= 16 {
                current.push(p);
            } else {
                if current.len() >= 2 {
                    groups.push(current.clone());
                }
                current = vec![p];
            }
        }
        if current.len() >= 2 {
            groups.push(current);
        }

        for group in groups {
            if group.len() <= 1 {
                continue;
            }
            let mut infos: Vec<&PortInfo> = Vec::new();
            for port in &group {
                if let Some(v) = sorted_ports.get(port) {
                    infos.extend(v);
                }
            }
            for i in 0..infos.len() {
                for j in (i + 1)..infos.len() {
                    let a = infos[i];
                    let b = infos[j];
                    if a.project == b.project || a.port == b.port {
                        continue;
                    }
                    let id = Synergy::stable_id(
                        "ServiceBus",
                        &a.project,
                        &b.project,
                        &format!("bus-{}-{}", a.port.min(b.port), a.port.max(b.port)),
                    );
                    let auto = auto_score_from(2, 0.7, type_floor("ServiceBus"));
                    out.push(Synergy {
                        id: id.clone(),
                        synergy_type: "ServiceBus".into(),
                        source_project: a.project.clone(),
                        target_project: b.project.clone(),
                        description: format!(
                            "Services voisins : `{}:{}` et `{}:{}` — candidats à un bus commun",
                            a.project, a.port, b.project, b.port
                        ),
                        priority: priority_from_score(auto),
                        effort: "medium".into(),
                        value: "medium".into(),
                        auto_score: auto,
                        adjusted_score: auto,
                        evidence: vec![
                            Evidence {
                                project: a.project.clone(),
                                path: a.path.clone(),
                                line: None,
                                fragment: format!("port {} via {}", a.port, a.source),
                                weight: 0.7,
                            },
                            Evidence {
                                project: b.project.clone(),
                                path: b.path.clone(),
                                line: None,
                                fragment: format!("port {} via {}", b.port, b.source),
                                weight: 0.7,
                            },
                        ],
                        suggested_action: Some("Déployer NATS/MQTT comme bus central".into()),
                        fingerprint: blake3::hash(format!("bus:{}:{}", a.port, b.port).as_bytes())
                            .to_string(),
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
        out.truncate(40);
        Ok(out)
    }
}

fn project_owning(ctx: &DetectContext, path: &PathBuf) -> Option<String> {
    let canon = path.canonicalize().ok().unwrap_or_else(|| path.clone());
    for p in ctx.ecosystem.iter() {
        let pcanon = p.path.canonicalize().ok().unwrap_or_else(|| p.path.clone());
        if canon.starts_with(&pcanon) {
            return Some(p.name.clone());
        }
    }
    None
}
