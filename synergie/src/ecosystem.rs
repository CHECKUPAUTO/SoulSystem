//! Découverte de l'écosystème : pour chaque racine, repère les projets et
//! les classe (RustCrate/PythonPackage/...) avec leurs manifests/services.

use crate::config::Config;
use crate::types::{Language, Project, ProjectKind, ServiceHint};
use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const MANIFESTS_BY_KIND: &[(&str, ProjectKind)] = &[
    ("Cargo.toml", ProjectKind::RustCrate),
    ("package.json", ProjectKind::NodePackage),
    ("pyproject.toml", ProjectKind::PythonPackage),
    ("setup.py", ProjectKind::PythonPackage),
    ("requirements.txt", ProjectKind::PythonPackage),
    ("go.mod", ProjectKind::GoModule),
];

#[derive(Debug, Clone)]
pub struct Ecosystem {
    pub projects: Vec<Project>,
}

impl Ecosystem {
    pub fn discover(cfg: &Config) -> Result<Self> {
        let mut projects: Vec<Project> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        for root in &cfg.ecosystem.roots {
            if !root.exists() {
                debug!(root = %root.display(), "racine inexistante — skip");
                continue;
            }
            let walker = WalkBuilder::new(root)
                .max_depth(Some(cfg.ecosystem.max_depth))
                .standard_filters(cfg.ecosystem.respect_gitignore)
                .build();
            for entry in walker.filter_map(|e| e.ok()) {
                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    continue;
                }
                let path = entry.path();
                let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !MANIFESTS_BY_KIND.iter().any(|(n, _)| *n == fname) {
                    continue;
                }
                let proj_dir = match path.parent() {
                    Some(p) => p.to_path_buf(),
                    None => continue,
                };
                if seen.contains(&proj_dir) {
                    // Déjà identifié — on ajoute juste un manifest supplémentaire.
                    if let Some(p) = projects.iter_mut().find(|pp| pp.path == proj_dir) {
                        if !p.manifests.iter().any(|m| m == path) {
                            p.manifests.push(path.to_path_buf());
                            p.kind = upgrade_kind(p.kind, kind_of(fname));
                            p.tags = derive_tags(&p.path, &p.name, &p.kind);
                            p.services.extend(sniff_services(&proj_dir, fname));
                        }
                    }
                    continue;
                }
                seen.insert(proj_dir.clone());
                let name = proj_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                let kind = kind_of(fname);
                let tags = derive_tags(&proj_dir, &name, &kind);
                let services = sniff_services(&proj_dir, fname);
                let languages = detect_languages(&kind);

                projects.push(Project {
                    name,
                    path: proj_dir,
                    kind,
                    tags,
                    languages,
                    manifests: vec![path.to_path_buf()],
                    services,
                });
            }
        }

        info!(target: "ecosystem", n = projects.len(), "projets découverts");
        Ok(Self { projects })
    }
}

fn kind_of(manifest: &str) -> ProjectKind {
    MANIFESTS_BY_KIND
        .iter()
        .find(|(n, _)| *n == manifest)
        .map(|(_, k)| *k)
        .unwrap_or(ProjectKind::Unknown)
}

fn upgrade_kind(a: ProjectKind, b: ProjectKind) -> ProjectKind {
    if a == b {
        return a;
    }
    ProjectKind::Mixed
}

fn derive_tags(dir: &Path, name: &str, kind: &ProjectKind) -> Vec<String> {
    let mut tags = Vec::new();
    let lower = name.to_lowercase();
    let path_lower = dir.to_string_lossy().to_lowercase();
    for hint in [
        "soullink",
        "openclaw",
        "cortex",
        "avid",
        "aris",
        "openevolve",
        "memori",
        "tribe",
        "ncpu",
        "msa",
        "promptify",
        "ironreview",
        "turboquant",
        "synergie",
    ] {
        if lower.contains(hint) || path_lower.contains(hint) {
            tags.push(hint.to_string());
        }
    }
    match kind {
        ProjectKind::RustCrate | ProjectKind::RustWorkspace => tags.push("rust".into()),
        ProjectKind::PythonPackage => tags.push("python".into()),
        ProjectKind::NodePackage => tags.push("node".into()),
        ProjectKind::GoModule => tags.push("go".into()),
        ProjectKind::Mixed => tags.push("mixed".into()),
        _ => {}
    }
    tags
}

fn detect_languages(k: &ProjectKind) -> Vec<Language> {
    match k {
        ProjectKind::RustCrate | ProjectKind::RustWorkspace => vec![Language::Rust],
        ProjectKind::PythonPackage => vec![Language::Python],
        ProjectKind::NodePackage => vec![Language::JavaScript, Language::TypeScript],
        ProjectKind::GoModule => vec![Language::Go],
        ProjectKind::Mixed => vec![Language::Rust, Language::Python],
        _ => vec![],
    }
}

fn sniff_services(dir: &Path, manifest_name: &str) -> Vec<ServiceHint> {
    let mut out = Vec::new();
    let m_path = dir.join(manifest_name);
    let text = match std::fs::read_to_string(&m_path) {
        Ok(t) => t,
        Err(_) => return out,
    };
    // Mots-clés simples → kind ; port à scanner ailleurs.
    let hints: &[(&str, &str)] = &[
        ("axum", "axum"),
        ("actix-web", "actix"),
        ("hyper", "hyper"),
        ("tonic", "grpc"),
        ("fastapi", "fastapi"),
        ("flask", "flask"),
        ("django", "django"),
        ("express", "express"),
        ("koa", "koa"),
        ("gin-gonic", "gin"),
    ];
    let lower = text.to_lowercase();
    for (key, kind) in hints {
        if lower.contains(key) {
            out.push(ServiceHint {
                kind: kind.to_string(),
                source: manifest_name.to_string(),
                port: None,
            });
        }
    }
    // Cherche un éventuel port dans la config (regex).
    let re = regex::Regex::new(r"port\s*[:=]\s*(\d{2,5})").unwrap();
    for cap in re.captures_iter(&text) {
        if let Some(p) = cap.get(1).and_then(|m| m.as_str().parse::<u16>().ok()) {
            if let Some(last) = out.last_mut() {
                last.port = Some(p);
            } else {
                out.push(ServiceHint {
                    kind: "http".into(),
                    source: manifest_name.to_string(),
                    port: Some(p),
                });
            }
        }
    }
    out
}
