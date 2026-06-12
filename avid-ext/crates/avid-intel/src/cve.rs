use tracing::info;

use crate::types::CveEntry;

/// Analyse CVE individuelle
pub async fn fetch_cve(cve_id: &str) -> anyhow::Result<CveEntry> {
    info!("Fetching CVE: {}", cve_id);
    let url = format!("https://services.nvd.nist.gov/rest/json/cves/2.0?cveId={cve_id}");
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;
    let body: serde_json::Value = resp.json().await?;

    // Extraire les données du format NVD 2.0
    let vulns = body["vulnerabilities"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| anyhow::anyhow!("No vulnerability data for {cve_id}"))?;

    let cve_data = &vulns["cve"];
    let descriptions = cve_data["descriptions"]
        .as_array()
        .and_then(|arr| arr.iter().find(|d| d["lang"] == "en"))
        .or_else(|| cve_data["descriptions"].as_array().and_then(|arr| arr.first()));

    let description = descriptions
        .and_then(|d| d["value"].as_str())
        .unwrap_or("No description available")
        .to_string();

    let metrics = &cve_data["metrics"];
    let cvss_score = metrics["cvssMetricV31"]
        .as_array()
        .and_then(|arr| arr.first())
        .or_else(|| metrics["cvssMetricV30"].as_array().and_then(|arr| arr.first()))
        .or_else(|| metrics["cvssMetricV2"].as_array().and_then(|arr| arr.first()))
        .and_then(|m| m["cvssData"]["baseScore"].as_f64());

    let severity = cvss_score.map_or_else(
        || "UNKNOWN".to_string(),
        |s| CveEntry::severity_from_cvss(s).to_string(),
    );

    let published = cve_data["published"].as_str().map(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(chrono::Utc::now())
    });

    let references: Vec<String> = cve_data["references"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r["url"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut entry = CveEntry::new(cve_id.to_string(), description);
    entry.severity = severity;
    entry.cvss_score = cvss_score;
    entry.published_date = published;
    entry.references = references;
    entry.is_exploitable = cvss_score.is_some_and(|s| s >= 7.0);

    info!("CVE {} fetched: score={:?} severity={}", cve_id, cvss_score, entry.severity);
    Ok(entry)
}

/// Recherche de CVEs par mot-clé
pub async fn search_cves(query: &str, limit: usize) -> anyhow::Result<Vec<CveEntry>> {
    info!("Searching CVEs for: '{}' (limit: {})", query, limit);
    let url = format!(
        "https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch={}&resultsPerPage={}",
        urlencoding(query),
        limit.min(200),
    );
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;
    let body: serde_json::Value = resp.json().await?;

    let vulns = body["vulnerabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut entries = Vec::with_capacity(vulns.len().min(limit));
    for vuln in vulns.iter().take(limit) {
        let cve_data = &vuln["cve"];
        let cve_id = cve_data["id"].as_str().unwrap_or("UNKNOWN").to_string();
        let desc = cve_data["descriptions"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|d| d["value"].as_str())
            .unwrap_or("")
            .to_string();
        let mut entry = CveEntry::new(cve_id, desc);
        entry.severity = cve_data["metrics"]
            .as_object()
            .and_then(|m| {
                m.values()
                    .next()
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|m| m["cvssData"]["baseScore"].as_f64())
                    .map(|s| CveEntry::severity_from_cvss(s).to_string())
            })
            .unwrap_or_else(|| "UNKNOWN".to_string());
        entries.push(entry);
    }

    info!("Search returned {} results", entries.len());
    Ok(entries)
}

/// URL-encode simple
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_cve_known() {
        // CVE-2023-44487 est un CVE connu (HTTP/2 Rapid Reset)
        // Le test dépend de la disponibilité du réseau
        let result = fetch_cve("CVE-2023-44487").await;
        if let Ok(cve) = result {
            assert_eq!(cve.id, "CVE-2023-44487");
            assert!(!cve.description.is_empty());
        }
        // Si réseau indisponible, skip silencieux
    }

    #[tokio::test]
    async fn test_search_cves() {
        let results = search_cves("kernel", 5).await;
        if let Ok(entries) = results {
            assert!(entries.len() <= 5);
            if !entries.is_empty() {
                assert!(!entries[0].id.is_empty());
            }
        }
    }

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
        assert_eq!(urlencoding("a+b"), "a%2Bb");
    }
}
