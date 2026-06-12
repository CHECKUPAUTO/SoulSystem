use anyhow::Context;
use blake3::Hasher;
use sled::Db;

use crate::self_modify::patch_generator::PatchCandidate;
use crate::self_modify::scorer::PatchScore;

/// Persistent memory of past evolution attempts.
pub struct EvolutionMemory {
    db: Db,
}

impl EvolutionMemory {
    /// Open the default sled database under `~/.soullink/evolution_memory.sled`.
    pub fn default_db() -> anyhow::Result<Self> {
        Self::open(
            dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("soullink")
                .join("evolution_memory.sled"),
        )
    }

    /// Open a sled database at a custom path (useful for tests).
    pub fn open(path: std::path::PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&path)?;
        let db = sled::open(&path).context("open sled db")?;
        Ok(Self { db })
    }

    /// Check whether this candidate has been seen before.
    pub fn is_duplicate(&self, candidate: &PatchCandidate) -> anyhow::Result<bool> {
        let hash = Self::hash(candidate);
        Ok(self.db.contains_key(hash.as_bytes())?)
    }

    /// Persist a candidate and its score.
    pub fn store(&self, candidate: &PatchCandidate, score: &PatchScore) -> anyhow::Result<()> {
        let hash = Self::hash(candidate);
        let meta = serde_json::json!({
            "id": candidate.id,
            "strategy": candidate.strategy,
            "score": score.total,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.db.insert(hash.as_bytes(), meta.to_string().as_bytes())?;
        self.db.flush()?;
        Ok(())
    }

    fn hash(candidate: &PatchCandidate) -> String {
        let mut hasher = Hasher::new();
        hasher.update(candidate.diff.as_bytes());
        hasher.update(format!("{:?}", candidate.strategy).as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}
