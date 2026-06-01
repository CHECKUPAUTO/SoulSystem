use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

// ─────────────────────────────────────────────
// Résultat de crawl
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub url: String,
    pub title: String,
    pub content: String,
    pub links: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ─────────────────────────────────────────────
// Crawler asynchrone
// ─────────────────────────────────────────────

pub struct Crawler {
    client: Client,
    max_retries: u32,
    rate_limit_ms: u64,
}

impl Crawler {
    pub fn new(timeout_secs: u64, max_retries: u32) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("OpenClaw/0.1 (research crawler)")
            .build()?;

        Ok(Self {
            client,
            max_retries,
            rate_limit_ms: 1000,
        })
    }

    /// Crawl une URL avec retry automatique
    pub async fn crawl_url(&self, url: &str) -> Result<CrawlResult> {
        let mut last_error = None;

        for attempt in 0..self.max_retries {
            if attempt > 0 {
                let delay = Duration::from_millis(self.rate_limit_ms * (attempt as u64 + 1));
                tokio::time::sleep(delay).await;
                info!("Retry {} pour {}", attempt + 1, url);
            }

            match self.fetch_and_parse(url).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("Erreur crawl {} (tentative {}): {}", url, attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Crawl échoué pour {}", url)))
    }

    /// Fetch + parse d'une page
    async fn fetch_and_parse(&self, url: &str) -> Result<CrawlResult> {
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP {} pour {}", response.status(), url));
        }

        let body = response.text().await?;
        let document = Html::parse_document(&body);

        let title = self.extract_title(&document);
        let content = self.extract_content(&document);
        let links = self.extract_links(&document, url);

        Ok(CrawlResult {
            url: url.to_string(),
            title,
            content,
            links,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Extraire le titre de la page
    fn extract_title(&self, doc: &Html) -> String {
        let selector = Selector::parse("title").unwrap();
        doc.select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default()
    }

    /// Extraire le contenu textuel principal
    fn extract_content(&self, doc: &Html) -> String {
        let selectors = ["article", "main", "body"];

        for sel_str in &selectors {
            if let Ok(selector) = Selector::parse(sel_str) {
                if let Some(element) = doc.select(&selector).next() {
                    let text: String = element
                        .text()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");

                    if text.len() > 100 {
                        // Tronquer à ~2000 caractères pour rester dans les limites
                        return text.chars().take(2000).collect();
                    }
                }
            }
        }

        String::new()
    }

    /// Extraire les liens de la page
    fn extract_links(&self, doc: &Html, base_url: &str) -> Vec<String> {
        let selector = Selector::parse("a[href]").unwrap();
        let base = url::Url::parse(base_url).ok();

        doc.select(&selector)
            .filter_map(|el| {
                let href = el.value().attr("href")?;
                if let Some(ref base) = base {
                    base.join(href).ok().map(|u| u.to_string())
                } else {
                    Some(href.to_string())
                }
            })
            .filter(|link| link.starts_with("http"))
            .take(50) // limiter le nombre de liens
            .collect()
    }

    /// Crawl multiple URLs en parallèle (avec limite de concurrence)
    pub async fn crawl_batch(&self, urls: &[String], max_concurrent: usize) -> Vec<CrawlResult> {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut handles = Vec::new();

        for url in urls {
            let sem = semaphore.clone();
            let url = url.clone();
            let client = self.client.clone();
            let max_retries = self.max_retries;
            let rate_limit_ms = self.rate_limit_ms;

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                let crawler = Crawler {
                    client,
                    max_retries,
                    rate_limit_ms,
                };
                crawler.crawl_url(&url).await.ok()
            });

            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(Some(result)) = handle.await {
                results.push(result);
            }
        }

        results
    }
}
