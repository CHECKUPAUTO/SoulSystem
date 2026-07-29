use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use soulsystem_common::secrets::SecretString;

/// Entrée CVE provenant de NVD ou OSV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveEntry {
    pub id: String,
    pub description: String,
    pub severity: String,
    pub cvss_score: Option<f64>,
    pub affected_packages: Vec<String>,
    pub published_date: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    pub references: Vec<String>,
    pub is_exploitable: bool,
}

impl CveEntry {
    pub fn new(id: String, description: String) -> Self {
        Self {
            id,
            description,
            severity: "UNKNOWN".to_string(),
            cvss_score: None,
            affected_packages: Vec::new(),
            published_date: None,
            last_modified: None,
            references: Vec::new(),
            is_exploitable: false,
        }
    }

    /// Détermine la sévérité à partir du score CVSS
    pub fn severity_from_cvss(score: f64) -> &'static str {
        if score >= 9.0 {
            "CRITICAL"
        } else if score >= 7.0 {
            "HIGH"
        } else if score >= 4.0 {
            "MEDIUM"
        } else if score >= 0.1 {
            "LOW"
        } else {
            "UNKNOWN"
        }
    }
}

/// Alerte générée par le moniteur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveAlert {
    pub cve: CveEntry,
    pub detected_at: DateTime<Utc>,
    pub source: String,
    pub summary: String,
}

impl CveAlert {
    pub fn new(cve: CveEntry, source: String) -> Self {
        let summary = format!(
            "[{}] {} — {}",
            cve.severity,
            cve.id,
            cve.description.chars().take(120).collect::<String>()
        );
        Self {
            cve,
            detected_at: Utc::now(),
            source,
            summary,
        }
    }
}

/// Configuration du moniteur OSINT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub nvd_api_key: Option<SecretString>,
    pub check_interval_minutes: u64,
    pub max_results_per_check: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            nvd_api_key: None,
            check_interval_minutes: 60,
            max_results_per_check: 50,
        }
    }
}

/// État du moniteur (dernier check timestamp)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorState {
    pub last_check: Option<DateTime<Utc>>,
    pub known_cves: Vec<String>,
}

impl Default for MonitorState {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorState {
    pub fn new() -> Self {
        Self {
            last_check: None,
            known_cves: Vec::new(),
        }
    }
}
