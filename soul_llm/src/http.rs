//! Shared HTTP response handling for the providers (MED-001).
//!
//! Before this, none of the three providers looked at the response status:
//! every call was `.send().await?.json().await?`, so a 429 or a 503 reached
//! `serde_json` as an HTML error page and surfaced as
//! `LlmError::Serialization`. `LlmError::RateLimited` was constructed nowhere
//! outside a unit test. Retry classification is only as good as the error it
//! classifies, so the status check and the retry loop land together.
//!
//! One helper rather than sixteen call sites, so the providers cannot drift on
//! which status counts as transient.

use std::time::Duration;

use crate::error::{LlmError, Result};

/// `send()`, then [`ensure_success`].
///
/// An extension trait rather than a free function so the call sites stay
/// ordinary builder chains — `.send_checked().await?` in place of
/// `.send().await?` — and adding a new provider request cannot accidentally
/// skip the status check by being written the old way.
#[async_trait::async_trait]
pub(crate) trait SendChecked {
    async fn send_checked(self) -> Result<reqwest::Response>;
}

#[async_trait::async_trait]
impl SendChecked for reqwest::RequestBuilder {
    async fn send_checked(self) -> Result<reqwest::Response> {
        ensure_success(self.send().await?).await
    }
}

/// Fail on a non-2xx response, mapping the status onto a classified
/// [`LlmError`] and carrying any `Retry-After` through.
pub(crate) async fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let retry_after = parse_retry_after(resp.headers());
    // The body is consumed here, which is fine: an unsuccessful response has
    // no payload we would otherwise parse.
    let body = resp.text().await.unwrap_or_default();
    Err(LlmError::from_http_status(
        status.as_u16(),
        retry_after,
        &body,
    ))
}

/// Parse `Retry-After`, which RFC 9110 allows to be either a delay in seconds
/// or an HTTP-date.
///
/// Both forms are handled because the value can come from a proxy or CDN in
/// front of the provider rather than the provider itself. A date in the past
/// yields `Some(0)` — "retry now" — rather than being discarded, and an
/// unparseable value yields `None` so the caller falls back to its own curve
/// instead of treating garbage as an instruction.
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let raw = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();

    if let Ok(secs) = raw.parse::<u64>() {
        return Some(secs);
    }

    let when = chrono::DateTime::parse_from_rfc2822(raw).ok()?;
    let delta = when.timestamp() - chrono::Utc::now().timestamp();
    Some(delta.max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    fn headers(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn retry_after_accepts_the_seconds_form() {
        assert_eq!(parse_retry_after(&headers("30")), Some(30));
        assert_eq!(parse_retry_after(&headers("  7  ")), Some(7));
        assert_eq!(parse_retry_after(&headers("0")), Some(0));
    }

    #[test]
    fn retry_after_accepts_the_http_date_form() {
        let future = chrono::Utc::now() + chrono::Duration::seconds(120);
        let raw = future.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let parsed = parse_retry_after(&headers(&raw)).expect("date form should parse");
        // Second-resolution formatting plus clock movement between the two
        // `now()` reads makes an exact equality assertion flaky.
        assert!(
            (115..=121).contains(&parsed),
            "expected roughly 120s, got {parsed}"
        );
    }

    #[test]
    fn a_retry_after_date_in_the_past_means_retry_now_not_never() {
        let past = chrono::Utc::now() - chrono::Duration::seconds(600);
        let raw = past.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert_eq!(parse_retry_after(&headers(&raw)), Some(0));
    }

    #[test]
    fn an_unparseable_retry_after_is_ignored_rather_than_guessed_at() {
        assert_eq!(parse_retry_after(&headers("soon")), None);
        assert_eq!(parse_retry_after(&headers("")), None);
        assert_eq!(parse_retry_after(&headers("-5")), None);
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn status_mapping_separates_transient_from_permanent() {
        assert!(matches!(
            LlmError::from_http_status(429, Some(12), "slow down"),
            LlmError::RateLimited {
                retry_after: Some(12)
            }
        ));
        assert!(matches!(
            LlmError::from_http_status(503, None, "overloaded"),
            LlmError::ServiceUnavailable { status: 503, .. }
        ));
        assert!(matches!(
            LlmError::from_http_status(500, None, ""),
            LlmError::ServiceUnavailable { status: 500, .. }
        ));
        assert!(matches!(
            LlmError::from_http_status(401, None, "bad key"),
            LlmError::Auth(_)
        ));
        assert!(matches!(
            LlmError::from_http_status(404, None, "no such model"),
            LlmError::Provider(_)
        ));

        assert!(LlmError::from_http_status(429, None, "").is_retryable());
        assert!(LlmError::from_http_status(503, None, "").is_retryable());
        assert!(!LlmError::from_http_status(401, None, "").is_retryable());
        assert!(
            !LlmError::from_http_status(404, None, "").is_retryable(),
            "a missing model is not fixed by asking again"
        );
    }

    #[test]
    fn an_error_body_is_truncated_rather_than_logged_whole() {
        let huge = "x".repeat(10_000);
        let msg = LlmError::from_http_status(400, None, &huge).to_string();
        assert!(
            msg.len() < 700,
            "error body must be bounded, got {}",
            msg.len()
        );
    }

    /// Serve exactly one raw HTTP response, then close.
    ///
    /// A real socket rather than a hand-built `reqwest::Response`: the property
    /// under test is the *wiring* — that `send_checked` consults the status at
    /// all — and constructing a response directly would bypass the very step
    /// that was missing before MED-001.
    async fn serve_once(raw: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut discard = [0u8; 2048];
            let _ = socket.read(&mut discard).await;
            let _ = socket.write_all(raw.as_bytes()).await;
            let _ = socket.flush().await;
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn a_real_429_response_becomes_a_retryable_rate_limit_with_its_retry_after() {
        let url = serve_once(
            "HTTP/1.1 429 Too Many Requests\r\n\
             Retry-After: 9\r\n\
             Content-Length: 10\r\n\
             \r\n\
             rate limit",
        )
        .await;

        let err = reqwest::Client::new()
            .get(&url)
            .send_checked()
            .await
            .expect_err("a 429 must not be reported as success");

        assert!(
            matches!(
                err,
                LlmError::RateLimited {
                    retry_after: Some(9)
                }
            ),
            "before this change the body was parsed as JSON and a 429 surfaced \
             as a serialization error; got {err}"
        );
        assert!(err.is_retryable());
        assert_eq!(err.retry_after(), Some(Duration::from_secs(9)));
    }

    #[tokio::test]
    async fn a_real_503_response_becomes_a_retryable_service_error() {
        let url = serve_once(
            "HTTP/1.1 503 Service Unavailable\r\n\
             Content-Length: 10\r\n\
             \r\n\
             overloaded",
        )
        .await;

        let err = reqwest::Client::new()
            .get(&url)
            .send_checked()
            .await
            .expect_err("a 503 must not be reported as success");

        assert!(
            matches!(err, LlmError::ServiceUnavailable { status: 503, .. }),
            "got {err}"
        );
        assert!(err.is_retryable());
    }

    #[tokio::test]
    async fn a_real_401_response_is_permanent_and_never_retried() {
        let url = serve_once(
            "HTTP/1.1 401 Unauthorized\r\n\
             Content-Length: 7\r\n\
             \r\n\
             bad key",
        )
        .await;

        let err = reqwest::Client::new()
            .get(&url)
            .send_checked()
            .await
            .expect_err("a 401 must not be reported as success");

        assert!(matches!(err, LlmError::Auth(_)), "got {err}");
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn a_successful_response_passes_through_untouched() {
        let url = serve_once(
            "HTTP/1.1 200 OK\r\n\
             Content-Length: 14\r\n\
             \r\n\
             {\"ok\":\"value\"}",
        )
        .await;

        let resp = reqwest::Client::new()
            .get(&url)
            .send_checked()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["ok"], "value");
    }
}
