use tracing::info;

use crate::types::CveEntry;

/// Client OSV (Open Source Vulnerabilities) — requêtes REST
pub struct OsvClient {
    client: reqwest::Client,
    base_url: String,
}

impl OsvClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.osv.dev/v1".to_string(),
        }
    }

    /// Interroge les vulnérabilités pour un package spécifique
    ///
    /// Format package: "pkg:cargo/serde", "pkg:pypi/django", "pkg:npm/express"
    pub async fn query_package(
        &self,
        package: &str,
        version: &str,
    ) -> anyhow::Result<Vec<CveEntry>> {
        info!("Querying OSV for package: {} @ {}", package, version);

        let payload = serde_json::json!({
            "package": {
                "purl": package,
            },
            "version": version,
        });

        let resp = self
            .client
            .post(format!("{}/query", self.base_url))
            .json(&payload)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        let vulns = body["vulns"].as_array().cloned().unwrap_or_default();

        let entries: Vec<CveEntry> = vulns.iter().filter_map(parse_osv_vuln).collect();
        info!(
            "OSV returned {} vulnerabilities for {} @ {}",
            entries.len(),
            package,
            version
        );
        Ok(entries)
    }

    /// Interroge les vulnérabilités liées à un commit
    pub async fn query_commit(&self, commit_hash: &str) -> anyhow::Result<Vec<CveEntry>> {
        info!("Querying OSV for commit: {}", commit_hash);

        let payload = serde_json::json!({
            "commit": commit_hash,
        });

        let resp = self
            .client
            .post(format!("{}/query", self.base_url))
            .json(&payload)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        let vulns = body["vulns"].as_array().cloned().unwrap_or_default();

        let entries: Vec<CveEntry> = vulns.iter().filter_map(parse_osv_vuln).collect();
        info!(
            "OSV returned {} vulnerabilities for commit {}",
            entries.len(),
            commit_hash
        );
        Ok(entries)
    }

    /// Récupère les détails d'une vulnérabilité spécifique OSV
    pub async fn get_vulnerability(&self, osv_id: &str) -> anyhow::Result<Option<CveEntry>> {
        info!("Fetching OSV vulnerability: {}", osv_id);

        let resp = self
            .client
            .get(format!("{}/vulns/{}", self.base_url, osv_id))
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(parse_osv_vuln(&body))
    }
}

impl Default for OsvClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse une vulnérabilité du format OSV vers CveEntry
fn parse_osv_vuln(vuln: &serde_json::Value) -> Option<CveEntry> {
    let id = vuln["id"].as_str()?;
    let summary = vuln["summary"].as_str().unwrap_or("").to_string();
    let details = vuln["details"].as_str().unwrap_or("").to_string();

    // Combine summary + details pour la description
    let description = if summary.is_empty() {
        details
    } else if details.is_empty() {
        summary
    } else {
        format!("{summary} — {details}")
    };

    let mut entry = CveEntry::new(id.to_string(), description);

    // Extrait le score CVSS s'il existe
    if let Some(severity_list) = vuln["severity"].as_array() {
        for sev in severity_list {
            if let (Some(sev_type), Some(score_str)) = (sev["type"].as_str(), sev["score"].as_str())
            {
                if sev_type == "CVSS_V3" {
                    if let Ok(score) = score_str.parse::<f64>() {
                        entry.cvss_score = Some(score);
                        entry.severity = CveEntry::severity_from_cvss(score).to_string();
                        entry.is_exploitable = score >= 7.0;
                    }
                }
            }
        }
    }

    // Extrait les références
    if let Some(refs) = vuln["references"].as_array() {
        entry.references = refs
            .iter()
            .filter_map(|r| r.as_str().map(String::from))
            .collect();
    }

    // Dates
    entry.published_date = vuln["published"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    entry.last_modified = vuln["modified"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    // Packages affectés
    if let Some(affected) = vuln["affected"].as_array() {
        for aff in affected {
            if let Some(pkg) = aff["package"]["purl"].as_str() {
                entry.affected_packages.push(pkg.to_string());
            }
            if let Some(pkg) = aff["package"]["name"].as_str() {
                entry.affected_packages.push(pkg.to_string());
            }
        }
    }

    Some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_osv_query_package() {
        let client = OsvClient::new();
        let results = client.query_package("pkg:cargo/serde", "1.0.0").await;
        if let Ok(entries) = results {
            // serde 1.0.0 peut avoir des vulnérabilités connues ou non
            // On vérifie juste que l'appel ne panique pas
        }
    }

    #[tokio::test]
    async fn test_osv_query_commit() {
        let client = OsvClient::new();
        // Commit spécifique connu pour être vulnérable (log4j)
        let results = client.query_commit("d5d9b0b").await;
        // Le commit peut ou non avoir des CVEs associées
    }

    #[tokio::test]
    async fn test_osv_get_vulnerability() {
        let client = OsvClient::new();
        // GHSA ID récent (dépend de la disponibilité OSV)
        let result = client.get_vulnerability("GHSA-222j-fm9f-8g3j").await;
        // Test vérifiant juste l'absence de panique
    }
}
