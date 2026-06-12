use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Niveaux de sévérité d'un finding de sécurité
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl std::str::FromStr for Severity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(Severity::Critical),
            "high" => Ok(Severity::High),
            "medium" => Ok(Severity::Medium),
            "low" => Ok(Severity::Low),
            "info" => Ok(Severity::Info),
            _ => Err(()),
        }
    }
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
            Severity::Info => "info",
        }
    }
}

/// Niveau de confiance dans l'exploitabilité
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Certain,
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.9 {
            Confidence::Certain
        } else if score >= 0.7 {
            Confidence::High
        } else if score >= 0.4 {
            Confidence::Medium
        } else {
            Confidence::Low
        }
    }
}

/// Étapes du pipeline agentic
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PipelineStage {
    Understand,
    Validate,
    Exploit,
    Patch,
    Review,
}

/// Résultat d'analyse de sécurité
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub file_path: String,
    pub line_number: u32,
    pub description: String,
    pub is_exploitable: bool,
    pub confidence: Confidence,
    pub exploit_code: Option<String>,
    pub patch_code: Option<String>,
    pub cwe_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Finding {
    pub fn new(
        title: String,
        severity: Severity,
        file_path: String,
        line_number: u32,
        description: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            severity,
            file_path,
            line_number,
            description,
            is_exploitable: false,
            confidence: Confidence::Low,
            exploit_code: None,
            patch_code: None,
            cwe_id: None,
            created_at: Utc::now(),
        }
    }
}

/// Cible d'analyse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanTarget {
    pub path: String,
    pub language: String,
    pub is_binary: bool,
}

impl ScanTarget {
    pub fn new(path: String, language: String, is_binary: bool) -> Self {
        Self {
            path,
            language,
            is_binary,
        }
    }

    /// Détecte le langage à partir de l'extension de fichier
    pub fn detect_language(path: &str) -> String {
        if path.ends_with(".rs") {
            "rust".to_string()
        } else if path.ends_with(".py") {
            "python".to_string()
        } else if path.ends_with(".js") || path.ends_with(".ts") {
            "javascript".to_string()
        } else if path.ends_with(".go") {
            "go".to_string()
        } else if path.ends_with(".java") {
            "java".to_string()
        } else if path.ends_with(".c") || path.ends_with(".cpp") || path.ends_with(".h") {
            "c/cpp".to_string()
        } else {
            "unknown".to_string()
        }
    }
}

/// Rapport complet du pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineReport {
    pub target: ScanTarget,
    pub findings: Vec<Finding>,
    pub stages_completed: Vec<PipelineStage>,
    pub summary: String,
    pub duration_ms: u64,
}

impl PipelineReport {
    pub fn new(target: ScanTarget) -> Self {
        Self {
            target,
            findings: Vec::new(),
            stages_completed: Vec::new(),
            summary: String::new(),
            duration_ms: 0,
        }
    }
}
