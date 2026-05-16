//! Marketplace de skills pour Clawd.
//!
//! Permet de charger, installer et exécuter des skills
//! sous forme de bibliothèques dynamiques.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Trait qu'un skill doit implémenter.
/// En pratique, les skills sont chargés comme .so/.dylib.
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, input: &str) -> String;
}

/// Métadonnées d'un skill (fichier YAML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub entrypoint: String,
    pub dependencies: Vec<String>,
}

/// Chargeur de skills.
pub struct SkillLoader {
    skills_dir: PathBuf,
    skills: HashMap<String, Box<dyn Skill>>,
}

impl SkillLoader {
    /// Crée un loader pointant vers le répertoire de skills.
    pub fn new(skills_dir: &Path) -> Self {
        Self {
            skills_dir: skills_dir.to_path_buf(),
            skills: HashMap::new(),
        }
    }

    /// Charge tous les skills installés.
    pub fn load_all(&mut self) -> Result<usize> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir)?;
            return Ok(0);
        }

        let mut count = 0;
        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let meta_path = path.join("skill.yaml");
                if meta_path.exists() {
                    let meta: SkillMetadata =
                        serde_yaml::from_str(&std::fs::read_to_string(&meta_path)?)?;
                    info!(
                        "SkillLoader: loaded skill '{}' v{}",
                        meta.name, meta.version
                    );
                    // En production: charger la .so avec libloading
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Installe un skill depuis une archive tar.
    pub fn install(&mut self, url: &str) -> Result<()> {
        warn!(
            "SkillLoader: install from URL not yet implemented (URL: {})",
            url
        );
        // En production : télécharger, vérifier signature, extraire, charger
        Ok(())
    }

    /// Liste les skills installés.
    pub fn list(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }
}
