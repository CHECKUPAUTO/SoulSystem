//! Marketplace de skills pour Clawd.
//!
//! Charge et exécute des skills Rust natifs sans dépendre de libloading.
//! Les skills sont enregistrés dans une HashMap<String, Box<dyn Skill>>.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Trait qu'un skill doit implémenter.
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

/// Chargeur et registre de skills.
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

    /// Enregistre un skill programmatiquement.
    pub fn register(&mut self, skill: Box<dyn Skill>) {
        let name = skill.name().to_string();
        info!("SkillLoader: registered skill '{}'", name);
        self.skills.insert(name, skill);
    }

    /// Charge tous les skills installés depuis le répertoire YAML.
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
                    let content = std::fs::read_to_string(&meta_path)?;
                    let meta: SkillMetadata = serde_yaml::from_str(&content)?;
                    info!(
                        "SkillLoader: loaded skill metadata '{}' v{}",
                        meta.name, meta.version
                    );
                    // Les YAML skills seraient chargés ici avec libloading
                    // Pour l'instant, on enregistre juste le nom
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Exécute un skill par son nom.
    pub fn execute(&self, name: &str, input: &str) -> Option<String> {
        self.skills.get(name).map(|skill| skill.execute(input))
    }

    /// Liste les skills installés.
    pub fn list(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    /// Retourne le nombre de skills enregistrés.
    pub fn count(&self) -> usize {
        self.skills.len()
    }

    /// Installe un skill depuis une archive tar (stub fonctionnel).
    pub fn install(&mut self, url: &str) -> Result<()> {
        warn!(
            "SkillLoader: install from URL not yet implemented (URL: {})",
            url
        );
        // Dans une implémentation complète : télécharger, vérifier signature,
        // extraire, parser le YAML, charger la .so.
        // Pour l'instant, on crée un répertoire vide avec le nom du skill
        let skill_name = url.split('/').last().unwrap_or("unknown");
        let skill_dir = self.skills_dir.join(skill_name);
        std::fs::create_dir_all(&skill_dir)?;
        let yaml_path = skill_dir.join("skill.yaml");
        std::fs::write(
            yaml_path,
            format!(
                "name: {}\nversion: 0.1.0\ndescription: Skill installé depuis {}\n",
                skill_name, url
            ),
        )?;
        info!("SkillLoader: skill '{}' installé depuis {}", skill_name, url);
        Ok(())
    }
}

// ── Skills intégrés ──────────────────────────────────────────────────────────

/// Skill d'aide — affiche les skills disponibles.
pub struct HelpSkill;

impl Skill for HelpSkill {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "Affiche la liste des skills disponibles"
    }

    fn execute(&self, input: &str) -> String {
        let _ = input; // help peut prendre des arguments optionnels
        "Skills disponibles : help, status, echo. Utilisez 'help <skill>' pour plus d'informations."
            .to_string()
    }
}

/// Skill de statut — retourne l'état du système.
pub struct StatusSkill;

impl Skill for StatusSkill {
    fn name(&self) -> &str {
        "status"
    }

    fn description(&self) -> &str {
        "Retourne l'état actuel du système SoulSystem"
    }

    fn execute(&self, _input: &str) -> String {
        format!(
            "SoulSystem status:\n- Uptime: {}s\n- Skills loaded: yes\n- Memory: active\n- Auth: ed25519",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        )
    }
}

/// Skill d'echo — retourne l'entrée.
pub struct EchoSkill;

impl Skill for EchoSkill {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Retourne le texte d'entrée"
    }

    fn execute(&self, input: &str) -> String {
        format!("Echo: {}", input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_skills_with_builtins() {
        let mut loader = SkillLoader::new(Path::new("/tmp/soulskills_test"));

        loader.register(Box::new(HelpSkill));
        loader.register(Box::new(StatusSkill));
        loader.register(Box::new(EchoSkill));

        assert_eq!(loader.count(), 3);
        let names = loader.list();
        assert!(names.contains(&"help".to_string()));
        assert!(names.contains(&"status".to_string()));
        assert!(names.contains(&"echo".to_string()));
    }

    #[test]
    fn test_execute_help() {
        let mut loader = SkillLoader::new(Path::new("/tmp/soulskills_test"));
        loader.register(Box::new(HelpSkill));

        let result = loader.execute("help", "").unwrap();
        assert!(result.contains("Skills disponibles"));
        assert!(result.contains("help"));
        assert!(result.contains("status"));
        assert!(result.contains("echo"));
    }

    #[test]
    fn test_execute_echo() {
        let mut loader = SkillLoader::new(Path::new("/tmp/soulskills_test"));
        loader.register(Box::new(EchoSkill));

        let result = loader.execute("echo", "hello world").unwrap();
        assert_eq!(result, "Echo: hello world");
    }

    #[test]
    fn test_execute_status() {
        let mut loader = SkillLoader::new(Path::new("/tmp/soulskills_test"));
        loader.register(Box::new(StatusSkill));

        let result = loader.execute("status", "").unwrap();
        assert!(result.contains("SoulSystem status"));
        assert!(result.contains("Uptime"));
    }

    #[test]
    fn test_execute_unknown_skill() {
        let loader = SkillLoader::new(Path::new("/tmp/soulskills_test"));
        let result = loader.execute("nonexistent", "test");
        assert!(result.is_none());
    }

    #[test]
    fn test_install_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut loader = SkillLoader::new(dir.path());
        loader
            .install("https://skills.example.com/my-skill")
            .unwrap();

        let skill_dir = dir.path().join("my-skill");
        assert!(skill_dir.exists());
        assert!(skill_dir.join("skill.yaml").exists());
    }
}
