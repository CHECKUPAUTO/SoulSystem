//! Types partagés de SYNERGIE.
//!
//! `Synergy` est conservée à plat (source_project/target_project) pour
//! compatibilité avec rstdp.rs et detect/symbolic.rs existants.
//! Les nouveaux types `Project`, `ProjectKind`, `Language`, `ScanReport`
//! et `SynergyStatus` étendent le modèle sans rien casser.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════
//  TYPE Synergy — contrat v3 existant, inchangé
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synergy {
    pub id: String,
    pub synergy_type: String,
    pub source_project: String,
    pub target_project: String,
    pub description: String,
    pub priority: u8,
    pub effort: String,
    pub value: String,
    pub auto_score: f32,
    pub adjusted_score: f32,

    // Champs v3 additionnels — `#[serde(default)]` → rétro-compatibles.
    #[serde(default)]
    pub status: SynergyStatus,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub suggested_action: Option<String>,
    #[serde(default = "default_now")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default = "default_now")]
    pub last_seen: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub fingerprint: String,
}

fn default_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

impl Default for Synergy {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: String::new(),
            synergy_type: String::new(),
            source_project: String::new(),
            target_project: String::new(),
            description: String::new(),
            priority: 1,
            effort: "medium".to_string(),
            value: "medium".to_string(),
            auto_score: 0.0,
            adjusted_score: 0.0,
            status: SynergyStatus::Proposed,
            evidence: Vec::new(),
            suggested_action: None,
            created_at: now,
            last_seen: now,
            fingerprint: String::new(),
        }
    }
}

impl Synergy {
    pub fn synergy_type_label(&self) -> &str {
        &self.synergy_type
    }

    pub fn value_weight(&self) -> f32 {
        match self.value.to_lowercase().as_str() {
            "critical" => 4.0,
            "high" => 3.0,
            "medium" | "med" => 2.0,
            "low" => 1.0,
            _ => 2.0,
        }
    }

    pub fn effort_factor(&self) -> f32 {
        match self.effort.to_lowercase().as_str() {
            "hour" | "minute" => 0.5,
            "easy" | "day" => 1.0,
            "medium" | "med" | "week" => 2.0,
            "hard" | "month" => 3.0,
            "quarter" => 4.0,
            _ => 2.0,
        }
    }

    /// Identifiant déterministe (hex blake3 6 octets) à partir des paramètres
    /// stables d'une synergie. Utilisé pour la dédup et la persistance.
    pub fn stable_id(ty: &str, source: &str, target: &str, key: &str) -> String {
        let mut parts = [source.to_string(), target.to_string()];
        parts.sort();
        let seed = format!("{}|{}|{}|{}", ty, parts[0], parts[1], key);
        let h = blake3::hash(seed.as_bytes());
        hex::encode(&h.as_bytes()[..6])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SynergyStatus {
    #[default]
    Proposed,
    Investigating,
    InProgress,
    Implemented,
    Rejected,
    Stale,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Evidence {
    pub project: String,
    #[serde(default)]
    pub path: PathBuf,
    pub line: Option<usize>,
    pub fragment: String,
    pub weight: f32,
}

// ═══════════════════════════════════════════════════════════════════════════
//  TYPES NOUVEAUX — Project / Language / ScanReport
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectKind {
    RustCrate,
    RustWorkspace,
    PythonPackage,
    NodePackage,
    GoModule,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Shell,
    Toml,
    Yaml,
    Json,
    Markdown,
    Other,
}

impl Language {
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" | "pyi" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "go" => Language::Go,
            "sh" | "bash" => Language::Shell,
            "toml" => Language::Toml,
            "yaml" | "yml" => Language::Yaml,
            "json" => Language::Json,
            "md" | "markdown" => Language::Markdown,
            _ => Language::Other,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceHint {
    pub kind: String,   // "axum", "fastapi", "express", "tonic", …
    pub source: String, // "Cargo.toml", "pyproject.toml", …
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub kind: ProjectKind,
    pub tags: Vec<String>,
    #[serde(default)]
    pub languages: Vec<Language>,
    #[serde(default)]
    pub manifests: Vec<PathBuf>,
    #[serde(default)]
    pub services: Vec<ServiceHint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportStats {
    pub total: usize,
    pub by_type: HashMap<String, usize>,
    pub by_priority: HashMap<u8, usize>,
    pub by_status: HashMap<String, usize>,
    pub mean_adjusted_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub elapsed_ms: u64,
    pub projects: Vec<Project>,
    pub synergies: Vec<Synergy>,
    pub stats: ReportStats,
    #[serde(default)]
    pub languages: HashMap<String, usize>,
}

impl Default for ScanReport {
    fn default() -> Self {
        Self {
            generated_at: chrono::Utc::now(),
            elapsed_ms: 0,
            projects: vec![],
            synergies: vec![],
            stats: ReportStats::default(),
            languages: HashMap::new(),
        }
    }
}
