use cookie::{Cookie, CookieJar};
use std::time::Duration;
use url::Url;

/// Async HTTP client with bounded redirects, timeout, and cookie jar.
#[derive(Debug, Clone)]
pub struct HttpClient {
    pub client: reqwest::Client,
    jar: CookieJar,
}

/// Client-level errors.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("invalid URL scheme: {0}")]
    InvalidScheme(String),
    #[error("HTTP {status}: {body}")]
    HttpError { status: u16, body: String },
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

/// Raw HTTP response with metadata.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub charset: Option<String>,
    pub body: String,
    pub final_url: Url,
    pub cookies: Vec<String>,
}

impl HttpClient {
    /// Build a new client with 10s timeout and max 5 redirects.
    #[must_use]
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("valid client configuration");
        Self {
            client,
            jar: CookieJar::new(),
        }
    }

    /// Fetch the body of `url` as a UTF-8 string.
    ///
    /// Validates scheme, follows redirects (max 5), checks HTTP status,
    /// detects charset from `Content-Type`, and persists cookies.
    ///
    /// # Errors
    /// Returns [`ClientError::InvalidScheme`] for non-http(s) schemes,
    /// [`ClientError::HttpError`] for 4xx/5xx responses,
    /// or [`ClientError::Request`] on network failure.
    pub async fn fetch(&mut self, url: &Url) -> Result<HttpResponse, ClientError> {
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ClientError::InvalidScheme(url.scheme().to_string()));
        }

        let mut request = self.client.get(url.clone());
        request = request.header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        );
        request = request.header("Accept-Language", "en-US,en;q=0.5");
        request = request.header("Accept-Encoding", "gzip, deflate, br");

        // Add stored cookies for this domain.
        let cookie_header: Vec<String> = self
            .jar
            .iter()
            .filter(|c| domain_matches(url, c.domain()))
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect();
        if !cookie_header.is_empty() {
            request = request.header("Cookie", cookie_header.join("; "));
        }

        let response = request.send().await?;
        let status = response.status().as_u16();
        let final_url = response.url().clone();

        // Store cookies from response.
        let mut response_cookies = Vec::new();
        for cookie_str in response.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(s) = cookie_str.to_str() {
                response_cookies.push(s.to_string());
                if let Ok(c) = Cookie::parse(s) {
                    self.jar.add(c.into_owned());
                }
            }
        }

        let (content_type, charset) = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(parse_content_type)
            .unwrap_or((None, None));

        let body_bytes = response.bytes().await?;
        let body = if let Some(ref cs) = charset {
            crate::charset::decode(&body_bytes, cs)
        } else {
            String::from_utf8_lossy(&body_bytes).into_owned()
        };

        if status >= 400 {
            return Err(ClientError::HttpError {
                status,
                body: body.clone(),
            });
        }

        Ok(HttpResponse {
            status,
            content_type,
            charset,
            body,
            final_url,
            cookies: response_cookies,
        })
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

fn domain_matches(url: &Url, cookie_domain: Option<&str>) -> bool {
    let host = url.host_str().unwrap_or("");
    match cookie_domain {
        None => true,
        Some(d) => {
            let d = d.trim_start_matches('.');
            host == d || host.ends_with(d)
        }
    }
}

/// Parse `text/html; charset=utf-8` into (`content_type`, `charset`).
fn parse_content_type(header: &str) -> (Option<String>, Option<String>) {
    let mut parts = header.split(';');
    let ct = parts.next().map(|s| s.trim().to_lowercase());
    let charset = parts.find_map(|s| {
        let mut kv = s.trim().splitn(2, '=');
        let key = kv.next()?;
        let val = kv.next()?;
        if key.trim().eq_ignore_ascii_case("charset") {
            Some(val.trim().trim_matches('"').to_lowercase())
        } else {
            None
        }
    });
    (ct, charset)
}
