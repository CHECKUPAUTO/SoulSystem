//! Indexation FS d'un projet : fichiers par langage, manifests, configs, READMEs.

use crate::config::EcosystemCfg;
use crate::types::{Language, Project};
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct FileIndex {
    pub by_lang: HashMap<Language, Vec<PathBuf>>,
    pub manifests: Vec<PathBuf>,
    pub readmes: Vec<PathBuf>,
    pub configs: Vec<PathBuf>,
}

impl FileIndex {
    pub fn count_lang(&self, l: Language) -> usize {
        self.by_lang.get(&l).map(|v| v.len()).unwrap_or(0)
    }

    pub fn files_of(&self, l: Language) -> &[PathBuf] {
        self.by_lang.get(&l).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

const MANIFEST_NAMES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "go.mod",
];

const CONFIG_EXTS: &[&str] = &["toml", "yaml", "yml", "env", "json"];

pub fn index(project: &Project, cfg: &EcosystemCfg) -> FileIndex {
    let mut idx = FileIndex::default();
    let max_size = cfg.max_file_size as u64;

    let walker = WalkBuilder::new(&project.path)
        .max_depth(Some(cfg.max_depth))
        .standard_filters(cfg.respect_gitignore)
        .add_custom_ignore_filename(".synergieignore")
        .build();

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path().to_path_buf();
        let path_str = path.to_string_lossy();
        if cfg.ignore_patterns_matches(&path_str) {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > max_size {
                continue;
            }
        }

        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let lang = Language::from_ext(ext);

        idx.by_lang.entry(lang).or_default().push(path.clone());

        if MANIFEST_NAMES.contains(&fname) {
            idx.manifests.push(path.clone());
        }
        if fname.eq_ignore_ascii_case("README.md") || fname.eq_ignore_ascii_case("README") {
            idx.readmes.push(path.clone());
        }
        if CONFIG_EXTS.contains(&ext) || fname.starts_with(".env") {
            idx.configs.push(path.clone());
        }
    }

    idx
}
