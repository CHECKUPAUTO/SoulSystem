use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Un programme candidat dans la population évolutive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub id: String,
    pub code: String,
    pub score: f64,
    pub generation: u32,
    pub parent_id: Option<String>,
    pub island_id: usize,
    pub created_at: DateTime<Utc>,
    pub metrics: std::collections::HashMap<String, f64>,

    /// Source language (`"rust"`, `"python"`, …). Optional for backward
    /// compatibility with v4.1 saved JSON.
    #[serde(default)]
    pub language: Option<String>,

    /// How this program was produced (e.g. "seed", "creative",
    /// "structured:RenameVariable", "crossover", "repaired").
    #[serde(default)]
    pub provenance: Option<String>,
}

impl Program {
    pub fn new(code: String, generation: u32, parent_id: Option<String>, island_id: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            code,
            score: 0.0,
            generation,
            parent_id,
            island_id,
            created_at: Utc::now(),
            metrics: std::collections::HashMap::new(),
            language: None,
            provenance: None,
        }
    }

    /// Builder helper to set language fluently.
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Builder helper to set provenance fluently.
    pub fn with_provenance(mut self, p: impl Into<String>) -> Self {
        self.provenance = Some(p.into());
        self
    }

    pub fn is_valid(&self) -> bool {
        !self.code.trim().is_empty() && self.score.is_finite()
    }
}
