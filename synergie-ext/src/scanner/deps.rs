//! Extraction des dépendances depuis les manifests (Cargo/NPM/Python/Go).

use crate::types::Project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub project: String,
    pub manifest: PathBuf,
    pub ecosystem: String,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectDeps {
    pub project: String,
    pub manifests: Vec<PathBuf>,
    pub dependencies: Vec<Dependency>,
}

pub fn scan_project(project: &Project) -> ProjectDeps {
    let mut deps = Vec::new();
    for m in &project.manifests {
        let fname = m.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let text = match std::fs::read_to_string(m) {
            Ok(t) => t,
            Err(_) => continue,
        };
        match fname {
            "Cargo.toml" => parse_cargo(&project.name, m, &text, &mut deps),
            "package.json" => parse_npm(&project.name, m, &text, &mut deps),
            "pyproject.toml" => parse_pyproject(&project.name, m, &text, &mut deps),
            "requirements.txt" => parse_pip(&project.name, m, &text, &mut deps),
            "setup.py" => parse_setup_py(&project.name, m, &text, &mut deps),
            "go.mod" => parse_go_mod(&project.name, m, &text, &mut deps),
            _ => {}
        }
    }
    ProjectDeps {
        project: project.name.clone(),
        manifests: project.manifests.clone(),
        dependencies: deps,
    }
}

fn parse_cargo(project: &str, manifest: &Path, text: &str, out: &mut Vec<Dependency>) {
    let v: toml::Value = match toml::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };
    // [dependencies], [dev-dependencies], [build-dependencies], [workspace.dependencies]
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = v.get(section).and_then(|x| x.as_table()) {
            for (k, vv) in t {
                let version = vv.as_str().map(|s| s.to_string()).or_else(|| {
                    vv.as_table()
                        .and_then(|tt| tt.get("version"))
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                });
                out.push(Dependency {
                    project: project.to_string(),
                    manifest: manifest.to_path_buf(),
                    ecosystem: "cargo".into(),
                    name: k.clone(),
                    version,
                });
            }
        }
    }
    if let Some(ws) = v.get("workspace").and_then(|x| x.get("dependencies")).and_then(|x| x.as_table()) {
        for (k, vv) in ws {
            let version = vv.as_str().map(|s| s.to_string()).or_else(|| {
                vv.as_table()
                    .and_then(|tt| tt.get("version"))
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string())
            });
            out.push(Dependency {
                project: project.to_string(),
                manifest: manifest.to_path_buf(),
                ecosystem: "cargo".into(),
                name: k.clone(),
                version,
            });
        }
    }
}

fn parse_npm(project: &str, manifest: &Path, text: &str, out: &mut Vec<Dependency>) {
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(obj) = v.get(section).and_then(|x| x.as_object()) {
            for (k, vv) in obj {
                out.push(Dependency {
                    project: project.to_string(),
                    manifest: manifest.to_path_buf(),
                    ecosystem: "npm".into(),
                    name: k.clone(),
                    version: vv.as_str().map(|s| s.to_string()),
                });
            }
        }
    }
}

fn parse_pyproject(project: &str, manifest: &Path, text: &str, out: &mut Vec<Dependency>) {
    let v: toml::Value = match toml::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };
    // PEP 621
    if let Some(deps_arr) = v
        .get("project")
        .and_then(|x| x.get("dependencies"))
        .and_then(|x| x.as_array())
    {
        for d in deps_arr {
            if let Some(s) = d.as_str() {
                let (name, version) = split_pep_dep(s);
                out.push(Dependency {
                    project: project.to_string(),
                    manifest: manifest.to_path_buf(),
                    ecosystem: "pypi".into(),
                    name,
                    version,
                });
            }
        }
    }
    // Poetry
    if let Some(t) = v
        .get("tool")
        .and_then(|x| x.get("poetry"))
        .and_then(|x| x.get("dependencies"))
        .and_then(|x| x.as_table())
    {
        for (k, vv) in t {
            if k == "python" { continue; }
            let version = vv.as_str().map(|s| s.to_string()).or_else(|| {
                vv.as_table().and_then(|tt| tt.get("version")).and_then(|x| x.as_str()).map(|s| s.to_string())
            });
            out.push(Dependency {
                project: project.to_string(),
                manifest: manifest.to_path_buf(),
                ecosystem: "pypi".into(),
                name: k.clone(),
                version,
            });
        }
    }
}

fn split_pep_dep(s: &str) -> (String, Option<String>) {
    let mut name = String::new();
    let mut iter = s.chars().peekable();
    while let Some(&c) = iter.peek() {
        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
            name.push(c);
            iter.next();
        } else {
            break;
        }
    }
    let rest: String = iter.collect();
    let trimmed = rest.trim();
    let version = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
    (name, version)
}

fn parse_pip(project: &str, manifest: &Path, text: &str, out: &mut Vec<Dependency>) {
    for line in text.lines() {
        let l = line.split('#').next().unwrap_or("").trim();
        if l.is_empty() || l.starts_with("-r") || l.starts_with("--") || l.starts_with("-e") {
            continue;
        }
        let (name, version) = split_pep_dep(l);
        if name.is_empty() { continue; }
        out.push(Dependency {
            project: project.to_string(),
            manifest: manifest.to_path_buf(),
            ecosystem: "pypi".into(),
            name,
            version,
        });
    }
}

fn parse_setup_py(project: &str, manifest: &Path, text: &str, out: &mut Vec<Dependency>) {
    let re = regex::Regex::new(r#"install_requires\s*=\s*\[([^\]]*)\]"#).unwrap();
    if let Some(cap) = re.captures(text) {
        let list = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        for raw in list.split(',') {
            let s = raw.trim().trim_matches(|c: char| c == '"' || c == '\'');
            if s.is_empty() { continue; }
            let (name, version) = split_pep_dep(s);
            if name.is_empty() { continue; }
            out.push(Dependency {
                project: project.to_string(),
                manifest: manifest.to_path_buf(),
                ecosystem: "pypi".into(),
                name,
                version,
            });
        }
    }
}

fn parse_go_mod(project: &str, manifest: &Path, text: &str, out: &mut Vec<Dependency>) {
    let mut in_block = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("require (") {
            in_block = true;
            continue;
        }
        if l == ")" && in_block {
            in_block = false;
            continue;
        }
        let candidate = if let Some(rest) = l.strip_prefix("require ") {
            Some(rest.trim())
        } else if in_block {
            Some(l)
        } else {
            None
        };
        if let Some(s) = candidate {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 2 {
                out.push(Dependency {
                    project: project.to_string(),
                    manifest: manifest.to_path_buf(),
                    ecosystem: "go".into(),
                    name: parts[0].to_string(),
                    version: Some(parts[1].to_string()),
                });
            }
        }
    }
}
