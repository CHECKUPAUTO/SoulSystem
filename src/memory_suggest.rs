//! MemorySuggest — Suggestions semi-automatiques pour MEMORY.md.
//!
//! # Problème
//! Les faits importants doivent être écrits manuellement dans MEMORY.md
//! pour survivre aux compactions. Il n'y a pas de mécanisme d'auto-suggestion.
//!
//! # Solution
//! 1. Pendant la session, le ContinuousSummarizer détecte des faits nouveaux
//! 2. Ces faits sont écrits dans `pending_facts.md`
//! 3. Ce module propose une API pour valider/rejeter ces faits
//! 4. Une fois validés, les faits sont intégrés dans MEMORY.md via `apply_memory_updates.py`
//!
//! # Sécurité
//! Aucune modification de MEMORY.md sans validation humaine explicite,
//! sauf si un indicateur de confiance > 0.95 est atteint.

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MemorySuggestConfig {
    /// Répertoire des données (contient pending_facts.md)
    pub data_dir: PathBuf,
    /// Chemin vers MEMORY.md à modifier
    pub memory_path: PathBuf,
    /// Chemin vers le script d'application
    pub apply_script_path: PathBuf,
    /// Seuil de confiance pour auto-approbation
    pub auto_approve_threshold: f32,
}

impl Default for MemorySuggestConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/soulsystem/data"),
            memory_path: PathBuf::from("/root/.openclaw/workspace/MEMORY.md"),
            apply_script_path: PathBuf::from("/root/.openclaw/workspace/apply_memory_updates.sh"),
            auto_approve_threshold: 0.95,
        }
    }
}

// ── Suggestions ────────────────────────────────────────────────────

/// Gestionnaire de suggestions pour MEMORY.md.
pub struct MemorySuggest {
    config: MemorySuggestConfig,
}

impl MemorySuggest {
    pub fn new(config: MemorySuggestConfig) -> Self {
        Self { config }
    }

    /// Chemin vers le fichier des faits en attente.
    pub fn pending_facts_path(&self) -> PathBuf {
        self.config.data_dir.join("pending_facts.md")
    }

    /// Chemin vers le fichier MEMORY.md cible.
    pub fn memory_path(&self) -> &PathBuf {
        &self.config.memory_path
    }

    /// Lit les faits en attente depuis pending_facts.md.
    pub async fn read_pending_facts(&self) -> Result<Vec<PendingFact>> {
        let path = self.pending_facts_path();
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(e) => return Err(e.into()),
        };

        let mut facts = Vec::new();
        for line in content.lines() {
            // Format: "N. [ ] texte du fait"
            if let Some(text) = line
                .strip_prefix(|c: char| c.is_ascii_digit())
                .and_then(|s| s.strip_prefix(". [ ] "))
            {
                facts.push(PendingFact {
                    text: text.to_string(),
                    accepted: false,
                });
            } else if let Some(text) = line
                .strip_prefix(|c: char| c.is_ascii_digit())
                .and_then(|s| s.strip_prefix(". [x] "))
            {
                facts.push(PendingFact {
                    text: text.to_string(),
                    accepted: true,
                });
            }
        }

        Ok(facts)
    }

    /// Accepte un fait spécifique (par index).
    pub async fn accept_fact(&self, index: usize) -> Result<bool> {
        let facts = self.read_pending_facts().await?;
        if index >= facts.len() {
            return Ok(false);
        }

        // Réécrire le fichier avec la case cochée
        let mut content = String::from("# Faits en attente de validation\n\n");
        content.push_str("Valide avec `!memory accept <index>` ou `!memory accept all`.\n\n");

        for (i, fact) in facts.iter().enumerate() {
            let checked = if i == index || fact.accepted {
                "x"
            } else {
                " "
            };
            content.push_str(&format!("{}. [{}] {}\n", i + 1, checked, fact.text));
        }

        tokio::fs::write(self.pending_facts_path(), &content).await?;
        info!("MemorySuggest: fait #{} accepté", index);
        Ok(true)
    }

    /// Accepte tous les faits en attente.
    pub async fn accept_all(&self) -> Result<usize> {
        let facts = self.read_pending_facts().await?;
        if facts.is_empty() {
            return Ok(0);
        }

        // Marquer tous comme acceptés
        let mut content = String::from("# Faits en attente de validation\n\n");
        content.push_str("Tous les faits ci-dessous ont été acceptés.\n");
        content.push_str("Exécute `!memory apply` pour les intégrer dans MEMORY.md.\n\n");

        for (i, fact) in facts.iter().enumerate() {
            content.push_str(&format!("{}. [x] {}\n", i + 1, fact.text));
        }

        tokio::fs::write(self.pending_facts_path(), &content).await?;
        info!("MemorySuggest: {} faits acceptés", facts.len());
        Ok(facts.len())
    }

    /// Applique les faits acceptés à MEMORY.md.
    pub async fn apply_accepted(&self) -> Result<usize> {
        let facts = self.read_pending_facts().await?;
        let accepted: Vec<&PendingFact> = facts.iter().filter(|f| f.accepted).collect();

        if accepted.is_empty() {
            info!("MemorySuggest: aucun fait à appliquer");
            return Ok(0);
        }

        // Lire MEMORY.md existant
        let memory_content = match tokio::fs::read_to_string(&self.config.memory_path).await {
            Ok(c) => c,
            Err(_) => String::from("# MEMORY.md\n\n"),
        };

        // Ajouter les faits dans une section "Faits récents"
        let mut new_content = memory_content.trim_end().to_string();
        new_content.push_str("\n\n## 🧠 Faits récents (auto-suggestions)\n\n");

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        new_content.push_str(&format!("_Ajouté le {}_\n\n", today));

        for fact in &accepted {
            new_content.push_str(&format!("- [ ] {}\n", fact.text));
        }
        new_content.push('\n');

        // Écrire le nouveau MEMORY.md
        tokio::fs::write(&self.config.memory_path, &new_content).await?;

        // Nettoyer pending_facts.md
        tokio::fs::write(self.pending_facts_path(), "").await?;

        info!(
            "MemorySuggest: {} faits intégrés dans MEMORY.md",
            accepted.len()
        );
        Ok(accepted.len())
    }

    /// Vérifie si des faits en attente existent.
    pub async fn has_pending(&self) -> bool {
        match self.read_pending_facts().await {
            Ok(facts) => !facts.is_empty(),
            Err(_) => false,
        }
    }

    /// Compte les faits en attente.
    pub async fn pending_count(&self) -> usize {
        match self.read_pending_facts().await {
            Ok(facts) => facts.len(),
            Err(_) => 0,
        }
    }
}

// ── Types ──────────────────────────────────────────────────────────

/// Un fait en attente de validation.
#[derive(Debug, Clone)]
pub struct PendingFact {
    pub text: String,
    pub accepted: bool,
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_pending_facts() {
        let dir = TempDir::new().unwrap();
        let pending_path = dir.path().join("pending_facts.md");

        // Créer un fichier de faits en attente
        let content = "# Faits en attente\n\n1. [ ] Premier fait\n2. [x] Deuxième fait (accepté)\n3. [ ] Troisième fait\n";
        tokio::fs::write(&pending_path, content).await.unwrap();

        let config = MemorySuggestConfig {
            data_dir: dir.path().to_path_buf(),
            memory_path: dir.path().join("MEMORY.md"),
            ..Default::default()
        };
        let suggest = MemorySuggest::new(config);

        let facts = suggest.read_pending_facts().await.unwrap();
        assert_eq!(facts.len(), 3);
        assert!(!facts[0].accepted);
        assert!(facts[1].accepted);
        assert_eq!(facts[2].text, "Troisième fait");
    }

    #[tokio::test]
    async fn test_accept_fact() {
        let dir = TempDir::new().unwrap();
        let pending_path = dir.path().join("pending_facts.md");
        let content = "# Faits en attente\n\n1. [ ] Premier\n2. [ ] Deuxième\n";
        tokio::fs::write(&pending_path, content).await.unwrap();

        let config = MemorySuggestConfig {
            data_dir: dir.path().to_path_buf(),
            memory_path: dir.path().join("MEMORY.md"),
            ..Default::default()
        };
        let suggest = MemorySuggest::new(config);

        let accepted = suggest.accept_fact(0).await.unwrap();
        assert!(accepted);

        let facts = suggest.read_pending_facts().await.unwrap();
        assert!(facts[0].accepted);
        assert!(!facts[1].accepted);
    }

    #[tokio::test]
    async fn test_accept_all() {
        let dir = TempDir::new().unwrap();
        let pending_path = dir.path().join("pending_facts.md");
        let content = "# Faits en attente\n\n1. [ ] Fait A\n2. [ ] Fait B\n3. [ ] Fait C\n";
        tokio::fs::write(&pending_path, content).await.unwrap();

        let config = MemorySuggestConfig {
            data_dir: dir.path().to_path_buf(),
            memory_path: dir.path().join("MEMORY.md"),
            ..Default::default()
        };
        let suggest = MemorySuggest::new(config);

        let count = suggest.accept_all().await.unwrap();
        assert_eq!(count, 3);

        let facts = suggest.read_pending_facts().await.unwrap();
        assert!(facts.iter().all(|f| f.accepted));
    }

    #[tokio::test]
    async fn test_apply_to_memory() {
        let dir = TempDir::new().unwrap();
        let pending_path = dir.path().join("pending_facts.md");
        let memory_path = dir.path().join("MEMORY.md");

        // Créer MEMORY.md existant
        tokio::fs::write(&memory_path, "# MEMORY.md\n\n## Projets\n\n- Projet A\n")
            .await
            .unwrap();

        // Créer faits acceptés
        let content = "# Faits en attente\n\n1. [x] Fait important #1\n2. [x] Fait important #2\n3. [ ] Fait non accepté\n";
        tokio::fs::write(&pending_path, content).await.unwrap();

        let config = MemorySuggestConfig {
            data_dir: dir.path().to_path_buf(),
            memory_path: memory_path.clone(),
            ..Default::default()
        };
        let suggest = MemorySuggest::new(config);

        let count = suggest.apply_accepted().await.unwrap();
        assert_eq!(count, 2);

        // Vérifier que MEMORY.md a été mis à jour
        let memory = tokio::fs::read_to_string(&memory_path).await.unwrap();
        assert!(memory.contains("Fait important #1"));
        assert!(memory.contains("Fait important #2"));
        assert!(memory.contains("Faits récents"));
    }
}
