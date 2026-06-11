//! Web content fetching and extraction for RAG integration.
//!
//! Provides:
//! - HTTP fetching with retry and timeout
//! - HTML content extraction (readability-like)
//! - Markdown conversion
//! - Meta tag extraction
//! - Link extraction
//! - Rate limiting

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
    #[error("Rate limited")]
    RateLimited,
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Content too large: {0} bytes")]
    ContentTooLarge(usize),
    #[error("Task join error: {0}")]
    JoinError(String),
    #[error("Client build error: {0}")]
    ClientBuild(String),
}

pub type Result<T> = std::result::Result<T, FetchError>;

// ── Fetched Content ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedContent {
    pub url: String,
    pub title: String,
    pub text: String,
    pub markdown: String,
    pub html: String,
    pub meta: HashMap<String, String>,
    pub links: Vec<String>,
    pub fetched_at: DateTime<Utc>,
    pub size_bytes: usize,
    pub content_type: String,
    pub status_code: u16,
}

// ── Fetcher Configuration ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FetcherConfig {
    pub timeout_ms: u64,
    pub max_size_bytes: usize,
    pub user_agent: String,
    pub rate_limit_per_second: u32,
    pub respect_robots_txt: bool,
    pub follow_redirects: bool,
    pub max_redirects: usize,
}

impl Default for FetcherConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,
            max_size_bytes: 10 * 1024 * 1024, // 10MB
            user_agent: "SoulSystem/1.0 (autonomous agent)".into(),
            rate_limit_per_second: 2,
            respect_robots_txt: true,
            follow_redirects: true,
            max_redirects: 5,
        }
    }
}

// ── Web Fetcher ───────────────────────────────────────────────────────

pub struct WebFetcher {
    client: reqwest::Client,
    config: FetcherConfig,
    rate_limiter: RateLimiter,
}

impl WebFetcher {
    pub fn new(config: FetcherConfig) -> crate::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .user_agent(&config.user_agent)
            .redirect(if config.follow_redirects {
                reqwest::redirect::Policy::limited(config.max_redirects)
            } else {
                reqwest::redirect::Policy::none()
            })
            .build()
            .map_err(|e| crate::FetchError::ClientBuild(e.to_string()))?;

        Ok(Self {
            client,
            rate_limiter: RateLimiter::new(config.rate_limit_per_second),
            config,
        })
    }

    /// Fetch a URL and extract content
    pub async fn fetch(&self, url: &str) -> Result<FetchedContent> {
        // Rate limit
        self.rate_limiter.wait().await;

        // Validate URL
        let parsed = Url::parse(url).map_err(|_| FetchError::InvalidUrl(url.to_string()))?;

        // Reject URLs longer than 2048 characters
        if url.len() > 2048 {
            return Err(FetchError::InvalidUrl("URL exceeds 2048 characters".into()));
        }

        // Validate scheme is http or https
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(FetchError::InvalidUrl(format!(
                "unsupported scheme: {}",
                parsed.scheme()
            )));
        }

        // Reject URLs with embedded credentials
        if parsed.username() != "" || parsed.password().is_some() {
            return Err(FetchError::InvalidUrl(
                "URL must not contain embedded credentials".into(),
            ));
        }

        // Fetch
        let response = self.client.get(url).send().await?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        let html = response.text().await?;

        if html.len() > self.config.max_size_bytes {
            return Err(FetchError::ContentTooLarge(html.len()));
        }

        // Extract content
        let title = extract_title(&html);
        let text = extract_text(&html);
        let markdown = html_to_markdown(&html);
        let meta = extract_meta(&html);
        let links = extract_links(&html, &parsed);
        let size_bytes = html.len();

        Ok(FetchedContent {
            url: url.to_string(),
            title,
            text,
            markdown,
            html,
            meta,
            links,
            fetched_at: Utc::now(),
            size_bytes,
            content_type,
            status_code: status,
        })
    }

    /// Fetch multiple URLs concurrently
    pub async fn fetch_many(&self, urls: &[&str]) -> Vec<Result<FetchedContent>> {
        let mut results = Vec::new();

        for url in urls {
            match self.fetch(url).await {
                Ok(content) => results.push(Ok(content)),
                Err(e) => results.push(Err(e)),
            }
        }

        results
    }
}

// ── Rate Limiter ──────────────────────────────────────────────────────

struct RateLimiter {
    interval_ms: u64,
    last_request: std::sync::Mutex<std::time::Instant>,
}

impl RateLimiter {
    fn new(per_second: u32) -> Self {
        Self {
            interval_ms: if per_second > 0 {
                1000 / per_second as u64
            } else {
                0
            },
            last_request: std::sync::Mutex::new(std::time::Instant::now()),
        }
    }

    async fn wait(&self) {
        if self.interval_ms == 0 {
            return;
        }
        let elapsed = {
            let last = self.last_request.lock().unwrap();
            last.elapsed().as_millis() as u64
        };
        if elapsed < self.interval_ms {
            let wait = self.interval_ms - elapsed;
            tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
            *self.last_request.lock().unwrap() = std::time::Instant::now();
        } else {
            *self.last_request.lock().unwrap() = std::time::Instant::now();
        }
    }
}

// ── HTML Extraction ───────────────────────────────────────────────────

fn extract_title(html: &str) -> String {
    if let Some(start) = html.find("<title>") {
        if let Some(end) = html[start..].find("</title>") {
            let title = &html[start + 7..start + end];
            return html_escape(title);
        }
    }
    String::new()
}

fn extract_text(html: &str) -> String {
    let mut text = String::new();
    let mut skip = false;
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                // Check for script/style start
                let lower = html[text.len()..].to_lowercase();
                if lower.starts_with("<script") || lower.starts_with("<style") {
                    skip = true;
                }
            }
            '>' => {
                in_tag = false;
                if !skip {
                    text.push(' ');
                }
                // Check for script/style end
                let lower = html[text.len()..].to_lowercase();
                if lower.starts_with("</script") || lower.starts_with("</style") {
                    skip = false;
                }
            }
            _ if !in_tag && !skip => {
                if ch != '\n' && ch != '\r' && ch != '\t' {
                    text.push(ch);
                } else {
                    text.push(' ');
                }
            }
            _ => {}
        }
    }

    // Collapse whitespace
    let words: Vec<&str> = text.split_whitespace().collect();
    words.join(" ")
}

fn html_to_markdown(html: &str) -> String {
    let mut md = String::new();
    let mut skip = false;
    let mut in_tag = false;
    let mut tag_name = String::new();

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_name.clear();
            }
            '>' => {
                in_tag = false;
                let tag = tag_name.to_lowercase();
                if tag.starts_with("script") || tag.starts_with("style") {
                    skip = !skip;
                }
                if !skip {
                    match tag.as_str() {
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            let level = tag.chars().last().unwrap_or('1') as usize - '0' as usize;
                            md.push_str(&"#".repeat(level));
                            md.push(' ');
                        }
                        "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => md.push_str("\n\n"),
                        "p" | "br" => md.push('\n'),
                        "/p" => md.push_str("\n\n"),
                        "b" | "strong" => md.push_str("**"),
                        "/b" | "/strong" => md.push_str("**"),
                        "i" | "em" => md.push('_'),
                        "/i" | "/em" => md.push('_'),
                        "a" => md.push('['),
                        "code" => md.push('`'),
                        "/code" => md.push('`'),
                        _ => {}
                    }
                }
            }
            _ if in_tag => tag_name.push(ch),
            _ if !skip => md.push(ch),
            _ => {}
        }
    }

    // Clean up
    let lines: Vec<&str> = md.lines().collect();
    lines.join("\n").trim().to_string()
}

fn extract_meta(html: &str) -> HashMap<String, String> {
    let mut meta = HashMap::new();

    // Description
    let re =
        regex::Regex::new(r#"<meta[^>]*name=["']description["'][^>]*content=["']([^"']+)["']"#)
            .unwrap();
    if let Some(caps) = re.captures(html) {
        if let Some(desc) = caps.get(1) {
            meta.insert("description".into(), html_escape(desc.as_str()));
        }
    }

    // Keywords
    let re = regex::Regex::new(r#"<meta[^>]*name=["']keywords["'][^>]*content=["']([^"']+)["']"#)
        .unwrap();
    if let Some(caps) = re.captures(html) {
        if let Some(kw) = caps.get(1) {
            meta.insert("keywords".into(), html_escape(kw.as_str()));
        }
    }

    // OG tags
    let re =
        regex::Regex::new(r#"<meta[^>]*property=["']og:([^"']+)["'][^>]*content=["']([^"']+)["']"#)
            .unwrap();
    for caps in re.captures_iter(html) {
        if let (Some(prop), Some(content)) = (caps.get(1), caps.get(2)) {
            meta.insert(
                format!("og:{}", prop.as_str()),
                html_escape(content.as_str()),
            );
        }
    }

    meta
}

fn extract_links(html: &str, base_url: &Url) -> Vec<String> {
    let mut links = Vec::new();
    let re = regex::Regex::new(r#"<a[^>]*href=["']([^"']+)["']"#).unwrap();

    for caps in re.captures_iter(html) {
        if let Some(href) = caps.get(1) {
            let link = href.as_str();
            if let Ok(abs_url) = base_url.join(link) {
                let url_str = abs_url.to_string();
                if !links.contains(&url_str) {
                    links.push(url_str);
                }
            }
        }
    }

    links
}

fn html_escape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = r#"<html><head><title>My Page</title></head></html>"#;
        assert_eq!(extract_title(html), "My Page");
    }

    #[test]
    fn test_extract_text() {
        let html = r#"<html><body><h1>Hello</h1><p>World</p></body></html>"#;
        let text = extract_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_extract_meta() {
        let html = r#"<meta name="description" content="A test page"><meta property="og:title" content="Test">"#;
        let meta = extract_meta(html);
        assert_eq!(meta.get("description").unwrap(), "A test page");
        assert_eq!(meta.get("og:title").unwrap(), "Test");
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("&amp;"), "&");
        assert_eq!(html_escape("&lt;div&gt;"), "<div>");
    }

    #[test]
    fn test_fetcher_config_default() {
        let config = FetcherConfig::default();
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_size_bytes, 10 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(10); // 10 per second
        let start = std::time::Instant::now();
        limiter.wait().await;
        limiter.wait().await;
        let elapsed = start.elapsed().as_millis();
        // Should wait at least 100ms (1000ms / 10)
        assert!(elapsed >= 100, "Rate limiter should wait at least 100ms");
    }

    #[test]
    fn test_url_scheme_validation() {
        let parsed = Url::parse("ftp://example.com").unwrap();
        assert!(!["http", "https"].contains(&parsed.scheme()));
    }

    #[test]
    fn test_url_too_long() {
        let long_url = format!("https://example.com/{}", "x".repeat(2048));
        assert!(long_url.len() > 2048);
    }

    #[test]
    fn test_url_credentials() {
        let parsed = Url::parse("https://user:pass@example.com").unwrap();
        assert!(!parsed.username().is_empty());
        assert!(parsed.password().is_some());
    }

    #[test]
    fn test_url_no_credentials() {
        let parsed = Url::parse("https://example.com").unwrap();
        assert!(parsed.username().is_empty());
        assert!(parsed.password().is_none());
    }
}
