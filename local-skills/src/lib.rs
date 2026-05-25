//! Local Skills — Chargement securise de plugins natifs (.so).
//!
//! Chaque skill est une bibliotheque dynamique (.so) accompagnee
//! d'une signature SHA-256-HMAC (.sig). La cle publique (identifiant)
//! est integree dans le fichier .sig.
//! La signature est verifiee avant tout chargement via la cle privee
//! enregistree dans le registre `authorized_keys`.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Signature verifiable d'un skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSignature {
    pub name: String,
    pub sha256: String,
    pub signature: Vec<u8>,
    /// Identifiant de la cle: SHA256(private_key)
    pub key_id: Vec<u8>,
}

/// Trait implemente par chaque skill.
pub trait LocalSkill: Send + Sync {
    fn name(&self) -> &str;
    fn execute(&self, input: &str) -> Result<String>;
}

/// Un skill charge et verifie.
pub struct LoadedSkill {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
}

/// Gestionnaire de skills locaux.
pub struct LocalSkillLoader {
    skills_dir: PathBuf,
    /// (skill_name, key_id) -> private_key
    authorized_keys: HashMap<(String, Vec<u8>), Vec<u8>>,
    skills: Vec<LoadedSkill>,
}

impl LocalSkillLoader {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            authorized_keys: HashMap::new(),
            skills: Vec::new(),
        }
    }

    pub fn default_skills_dir() -> PathBuf {
        dirs_home().join(".soulsystem").join("skills")
    }

    /// Enregistre une cle privee autorisee pour un skill.
    /// `key_id` = SHA256(private_key), sert d'identifiant public.
    pub fn authorize(&mut self, skill_name: &str, key_id: Vec<u8>, private_key: Vec<u8>) {
        self.authorized_keys
            .insert((skill_name.to_string(), key_id), private_key);
    }

    /// Scan le repertoire de skills et verifie les signatures.
    pub fn discover(&mut self) -> Result<usize> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir)?;
            return Ok(0);
        }

        let mut loaded = 0usize;
        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "so") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let sig_path = path.with_extension("so.sig");

                if !sig_path.exists() {
                    tracing::warn!("Skill {}: missing .sig file, skipped", name);
                    continue;
                }

                match self.verify_skill(&name, &path, &sig_path) {
                    Ok(true) => {
                        tracing::info!("Skill {}: verified and loaded", name);
                        self.skills.push(LoadedSkill {
                            name: name.clone(),
                            path: path.clone(),
                            enabled: true,
                        });
                        loaded += 1;
                    }
                    Ok(false) => {
                        tracing::warn!("Skill {}: signature verification failed", name);
                    }
                    Err(e) => {
                        tracing::error!("Skill {}: verification error: {}", name, e);
                    }
                }
            }
        }

        Ok(loaded)
    }

    fn verify_skill(&self, name: &str, so_path: &Path, sig_path: &Path) -> Result<bool> {
        let so_data = std::fs::read(so_path)?;
        let sig_json = std::fs::read_to_string(sig_path)?;
        let sig: SkillSignature = serde_json::from_str(&sig_json)?;

        if sig.name != name {
            bail!(
                "Signature name mismatch: expected '{}', got '{}'",
                name,
                sig.name
            );
        }

        let computed_hash = format!("{:x}", Sha256::digest(&so_data));
        if computed_hash != sig.sha256 {
            bail!("SO hash mismatch for skill '{}'", name);
        }

        // Look up private key by (name, key_id)
        let private_key = self
            .authorized_keys
            .get(&(name.to_string(), sig.key_id.clone()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No authorized key for skill '{}' with key_id {:02x?}",
                    name,
                    &sig.key_id[..8.min(sig.key_id.len())]
                )
            })?;

        // sig.signature = SHA256(sha256 || private_key)
        let mut hasher = Sha256::new();
        hasher.update(sig.sha256.as_bytes());
        hasher.update(private_key);
        let expected_sig = hasher.finalize().to_vec();

        Ok(expected_sig == sig.signature)
    }

    /// Signe un fichier .so. Genere .so.sig. Ne genere pas de .pub separe.
    pub fn sign_skill(so_path: &Path, private_key: &[u8], skill_name: &str) -> Result<()> {
        let so_data = std::fs::read(so_path)?;
        let sha256 = format!("{:x}", Sha256::digest(&so_data));

        // sig = SHA256(sha256 || private_key)
        let mut hasher = Sha256::new();
        hasher.update(sha256.as_bytes());
        hasher.update(private_key);
        let signature = hasher.finalize().to_vec();

        // key_id = SHA256(private_key) — identifiant public
        let key_id = Sha256::digest(private_key).to_vec();

        let sig = SkillSignature {
            name: skill_name.to_string(),
            sha256,
            signature,
            key_id,
        };

        let sig_path = so_path.with_extension("so.sig");
        std::fs::write(&sig_path, serde_json::to_string_pretty(&sig)?)?;

        Ok(())
    }

    pub fn loaded_skills(&self) -> &[LoadedSkill] {
        &self.skills
    }

    pub fn has_skill(&self, name: &str) -> bool {
        self.skills.iter().any(|s| s.name == name && s.enabled)
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"))
}

// ── Echo skill (built-in example) ───────────────────────────────────────

pub struct EchoSkill;

impl LocalSkill for EchoSkill {
    fn name(&self) -> &str {
        "echo"
    }
    fn execute(&self, input: &str) -> Result<String> {
        Ok(format!("ECHO: {}", input))
    }
}

pub struct BuiltinSkills {
    skills: HashMap<String, Box<dyn LocalSkill>>,
}

impl Default for BuiltinSkills {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinSkills {
    pub fn new() -> Self {
        let mut skills: HashMap<String, Box<dyn LocalSkill>> = HashMap::new();
        skills.insert("echo".into(), Box::new(EchoSkill));
        Self { skills }
    }

    pub fn execute(&self, name: &str, input: &str) -> Result<String> {
        match self.skills.get(name) {
            Some(skill) => skill.execute(input),
            None => bail!("Skill '{}' not found in builtins", name),
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sign_and_verify_skill() {
        let tmp = TempDir::new().unwrap();
        let so_path = tmp.path().join("test_skill.so");
        std::fs::write(&so_path, b"fake .so content for testing").unwrap();

        let private_key = b"my-secret-key-for-signing-skills!!";
        let key_id = Sha256::digest(private_key).to_vec();

        LocalSkillLoader::sign_skill(&so_path, private_key, "test_skill").unwrap();

        assert!(tmp.path().join("test_skill.so.sig").exists());

        let mut loader = LocalSkillLoader::new(tmp.path().to_path_buf());
        loader.authorize("test_skill", key_id, private_key.to_vec());
        let count = loader.discover().unwrap();
        assert_eq!(count, 1);
        assert!(loader.has_skill("test_skill"));
    }

    #[test]
    fn test_verify_rejects_tampered_skill() {
        let tmp = TempDir::new().unwrap();
        let so_path = tmp.path().join("tampered.so");
        std::fs::write(&so_path, b"original content").unwrap();

        let private_key = b"secret-key-32-bytes-long!!!!!!!";
        let key_id = Sha256::digest(private_key).to_vec();

        LocalSkillLoader::sign_skill(&so_path, private_key, "tampered").unwrap();

        // Modifier le .so
        std::fs::write(&so_path, b"tampered content").unwrap();

        let mut loader = LocalSkillLoader::new(tmp.path().to_path_buf());
        loader.authorize("tampered", key_id, private_key.to_vec());
        let count = loader.discover().unwrap();
        assert_eq!(count, 0, "tampered skill should not be loaded");
    }

    #[test]
    fn test_echo_builtin_skill() {
        let skills = BuiltinSkills::new();
        assert!(skills.has("echo"));
        let result = skills.execute("echo", "hello world").unwrap();
        assert_eq!(result, "ECHO: hello world");
    }

    #[test]
    fn test_builtin_skill_not_found() {
        let skills = BuiltinSkills::new();
        let result = skills.execute("nonexistent", "test");
        assert!(result.is_err());
    }
}
