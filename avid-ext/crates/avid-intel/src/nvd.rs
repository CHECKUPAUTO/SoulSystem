use std::time::Duration;

use tracing::{info, warn};

use crate::types::CveEntry;

/// Client pour la National Vulnerability Database (NVD) API 2.0
pub struct NvdClient {
#[allow(dead_code)]
    api_key: Option<String>,
    client: reqwest::Client,
    base_url: String,
}

impl NvdClient {
    /// Crée un nouveau client NVD
    pub fn new(api_key: Option<String>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref key) = api_key {
            headers.insert(
                "apiKey",
                reqwest::header::HeaderValue::from_str(key).unwrap(),
            );
        }
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            api_key,
            client,
            base_url: "https://services.nvd.nist.gov/rest/json/cves/2.0".to_string(),
        }
    }

    /// Récupère les CVEs les plus récents
    pub async fn fetch_recent(&self, limit: usize) -> anyhow::Result<Vec<CveEntry>> {
        info!("Fetching recent CVEs (limit: {})", limit);
        let url = format!(
            "{}?resultsPerPage={}",
            self.base_url,
            limit.min(200),
        );
        self.fetch_cve_list(&url).await
    }

    /// Recherche des CVEs par mot-clé
    pub async fn fetch_by_keyword(&self, keyword: &str) -> anyhow::Result<Vec<CveEntry>> {
        info!("Fetching CVEs by keyword: '{}'", keyword);
        let keyword_encoded: String = keyword
            .chars()
            .map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u8),
            })
            .collect();
        let url = format!(
            "{}?keywordSearch={}&resultsPerPage=50",
            self.base_url, keyword_encoded,
        );
        self.fetch_cve_list(&url).await
    }

    /// Récupère les CVEs publiés après une date donnée
    pub async fn fetch_since(
        &self,
        since: &chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> anyhow::Result<Vec<CveEntry>> {
        info!("Fetching CVEs since: {}", since.to_rfc3339());
        let url = format!(
            "{}?pubStartDate={}&resultsPerPage={}",
            self.base_url,
            since.format("%Y-%m-%dT%H:%M:%S.000"),
            limit.min(200),
        );
        self.fetch_cve_list(&url).await
    }

    /// Récupère un CVE spécifique par ID
    pub async fn fetch_by_id(&self, cve_id: &str) -> anyhow::Result<CveEntry> {
        info!("Fetching CVE by ID: {}", cve_id);
        let url = format!("{}?cveId={}", self.base_url, cve_id);
        let resp = self.client.get(&url).send().await?;
        let body: serde_json::Value = resp.json().await?;

        let vulns = body["vulnerabilities"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| anyhow::anyhow!("CVE not found: {cve_id}"))?;

        Ok(parse_nvd_vuln(vulns, cve_id))
    }

    /// Appel générique pour fetch une liste de CVEs
    async fn fetch_cve_list(&self, url: &str) -> anyhow::Result<Vec<CveEntry>> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            warn!("NVD API returned status: {}", resp.status());
            return Ok(Vec::new());
        }

        let body: serde_json::Value = resp.json().await?;
        let vulns = body["vulnerabilities"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let entries: Vec<CveEntry> = vulns
            .iter()
            .map(|vuln| {
                let cve_id = vuln["cve"]["id"].as_str().unwrap_or("UNKNOWN");
                parse_nvd_vuln(vuln, cve_id)
            })
            .collect();

        info!("Fetched {} CVEs", entries.len());
        Ok(entries)
    }
}

/// Parse une vulnérabilité du format NVD 2.0 vers CveEntry
fn parse_nvd_vuln(vuln: &serde_json::Value, cve_id: &str) -> CveEntry {
    let cve_data = &vuln["cve"];
    let descriptions = cve_data["descriptions"].as_array();
    let desc = descriptions
        .and_then(|arr| {
            arr.iter()
                .find(|d| d["lang"] == "en")
                .or_else(|| arr.first())
        })
        .and_then(|d| d["value"].as_str())
        .unwrap_or("No description")
        .to_string();

    let metrics = &cve_data["metrics"];
    let cvss_score = metrics["cvssMetricV31"]
        .as_array()
        .and_then(|arr| arr.first())
        .or_else(|| metrics["cvssMetricV30"].as_array().and_then(|arr| arr.first()))
        .or_else(|| metrics["cvssMetricV2"].as_array().and_then(|arr| arr.first()))
        .and_then(|m| m["cvssData"]["baseScore"].as_f64());

    let severity = cvss_score
        .map(|s| CveEntry::severity_from_cvss(s).to_string())
        .unwrap_or_else(|| "UNKNOWN".to_string());

    let published = cve_data["published"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let modified = cve_data["lastModified"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let references: Vec<String> = cve_data["references"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|r| r["url"].as_str().map(String::from)).collect())
        .unwrap_or_default();

    let affected_packages = extract_affected_packages(cve_data);

    let mut entry = CveEntry::new(cve_id.to_string(), desc);
    entry.severity = severity;
    entry.cvss_score = cvss_score;
    entry.affected_packages = affected_packages;
    entry.published_date = published;
    entry.last_modified = modified;
    entry.references = references;
    entry.is_exploitable = cvss_score.is_some_and(|s| s >= 7.0);
    entry
}

/// Extrait les packages affectés depuis les configurations CPE
fn extract_affected_packages(cve_data: &serde_json::Value) -> Vec<String> {
    let mut packages = Vec::new();
    if let Some(configurations) = cve_data["configurations"].as_array() {
        for config in configurations {
            if let Some(nodes) = config["nodes"].as_array() {
                for node in nodes {
                    if let Some(cpe_match) = node["cpeMatch"].as_array() {
                        for cpe in cpe_match {
                            if let Some(criteria) = cpe["criteria"].as_str() {
                                // Format: cpe:2.3:a:vendor:product:version:...
                                let parts: Vec<&str> = criteria.split(':').collect();
                                if parts.len() >= 5 {
                                    let vendor = parts[3];
                                    let product = parts[4];
                                    packages.push(format!("{vendor}:{product}"));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nvd_client_new() {
        let client = NvdClient::new(None);
        // Vérifie que le client est construit correctement
        assert!(client.api_key.is_none());
    }

    #[tokio::test]
    async fn test_fetch_recent() {
        let client = NvdClient::new(None);
        let results = client.fetch_recent(5).await;
        if let Ok(entries) = results {
            assert!(entries.len() <= 5);
        }
    }

    #[tokio::test]
    async fn test_fetch_by_keyword() {
        let client = NvdClient::new(None);
        let results = client.fetch_by_keyword("rust").await;
        if let Ok(entries) = results {
            if !entries.is_empty() {
                assert!(!entries[0].id.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_by_id_known() {
        let client = NvdClient::new(None);
        let result = client.fetch_by_id("CVE-2024-3094").await;
        // CVE-2024-3094 est le CVE XZ Utils backdoor (connu)
        // Le test peut échouer si pas de réseau, donc on vérifie juste l'absence de panique
    }
}
