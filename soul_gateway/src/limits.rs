//! CORS policy and request limits for the gateway.
//!
//! Closes INV-NET-4 and INV-NET-5.
//!
//! The router previously applied [`tower_http::cors::CorsLayer::permissive`] to
//! the *merged* router — so `Access-Control-Allow-Origin: *` covered the
//! authenticated `/v1/*` operator routes, not only `/health`. A permissive CORS
//! header does not itself bypass bearer authentication, but it removes the
//! browser's same-origin barrier in front of an operator API, which is the
//! wrong default for a surface that can run shell commands. No body,
//! concurrency or WebSocket-message limit existed on any path.
//!
//! Both are configured from the environment and both **fail closed**: an unset
//! CORS allowlist permits no cross-origin browser access at all, rather than
//! permitting every origin.

use std::time::Duration;

use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Maximum accepted request body, in bytes.
///
/// 1 MiB comfortably fits an operator request (a goal, a prompt, a shell
/// command) while bounding what an unauthenticated caller can make the process
/// buffer before authentication runs.
pub const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;

/// Maximum requests processed concurrently.
///
/// Excess requests wait rather than being rejected, so a burst degrades
/// latency instead of erroring, but the number in flight — and therefore the
/// work the process will do at once — is bounded.
pub const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 64;

/// Maximum accepted WebSocket message, in bytes.
///
/// `/v1/stream` is authenticated, but an authenticated client should still not
/// be able to make the server buffer an unbounded frame.
pub const DEFAULT_MAX_WS_MESSAGE_BYTES: usize = 256 * 1024;

/// Environment variable holding a comma-separated CORS origin allowlist.
pub const CORS_ALLOWLIST_VAR: &str = "SOULSYSTEM_GATEWAY_CORS_ORIGINS";

/// Read the configured limits, falling back to the defaults above.
///
/// A malformed value is treated as unset rather than as zero: `MAX_BODY=abc`
/// must not silently become "reject everything", which would look like an
/// outage rather than a misconfiguration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayLimits {
    pub max_body_bytes: usize,
    pub max_concurrent_requests: usize,
    pub max_ws_message_bytes: usize,
}

impl Default for GatewayLimits {
    fn default() -> Self {
        Self {
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_ws_message_bytes: DEFAULT_MAX_WS_MESSAGE_BYTES,
        }
    }
}

impl GatewayLimits {
    pub fn from_env() -> Self {
        let read = |var: &str, default: usize| -> usize {
            std::env::var(var)
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(default)
        };
        Self {
            max_body_bytes: read("SOULSYSTEM_GATEWAY_MAX_BODY_BYTES", DEFAULT_MAX_BODY_BYTES),
            max_concurrent_requests: read(
                "SOULSYSTEM_GATEWAY_MAX_CONCURRENT_REQUESTS",
                DEFAULT_MAX_CONCURRENT_REQUESTS,
            ),
            max_ws_message_bytes: read(
                "SOULSYSTEM_GATEWAY_MAX_WS_MESSAGE_BYTES",
                DEFAULT_MAX_WS_MESSAGE_BYTES,
            ),
        }
    }
}

/// The configured cross-origin policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsPolicy {
    /// No cross-origin browser access. The default when the allowlist is unset.
    ///
    /// This is not the same as "no CORS layer": a layer that emits no
    /// `Access-Control-Allow-Origin` is what makes a browser refuse the
    /// response, which is the intended restrictive behaviour.
    Disabled,
    /// Exactly these origins are permitted.
    Allowlist(Vec<String>),
}

impl CorsPolicy {
    /// Parse the allowlist from the environment.
    ///
    /// Unset, empty, or all-blank yields [`CorsPolicy::Disabled`] — the
    /// fail-closed direction. `*` is **not** accepted as a wildcard: allowing
    /// every origin has to be a deliberate enumeration, not a one-character
    /// config value that looks like a default.
    pub fn from_env() -> Self {
        match std::env::var(CORS_ALLOWLIST_VAR) {
            Ok(raw) => Self::parse(&raw),
            Err(_) => Self::Disabled,
        }
    }

    /// Parse a comma-separated origin list.
    pub fn parse(raw: &str) -> Self {
        let origins: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "*")
            .map(str::to_owned)
            .collect();
        if origins.is_empty() {
            Self::Disabled
        } else {
            Self::Allowlist(origins)
        }
    }

    /// Whether `origin` is permitted.
    pub fn allows(&self, origin: &str) -> bool {
        match self {
            Self::Disabled => false,
            Self::Allowlist(origins) => origins.iter().any(|o| o == origin),
        }
    }

    /// Build the tower-http layer for this policy.
    pub fn layer(&self) -> CorsLayer {
        let base = CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ])
            .max_age(Duration::from_secs(600));

        match self {
            // An empty origin list emits no Access-Control-Allow-Origin, so a
            // browser refuses the cross-origin response.
            Self::Disabled => base.allow_origin(AllowOrigin::list([])),
            Self::Allowlist(origins) => {
                let parsed: Vec<HeaderValue> = origins
                    .iter()
                    .filter_map(|o| HeaderValue::from_str(o).ok())
                    .collect();
                base.allow_origin(AllowOrigin::list(parsed))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_or_blank_allowlist_disables_cross_origin_access() {
        // The fail-closed direction: no configuration must not mean "any origin".
        assert_eq!(CorsPolicy::parse(""), CorsPolicy::Disabled);
        assert_eq!(CorsPolicy::parse("   "), CorsPolicy::Disabled);
        assert_eq!(CorsPolicy::parse(",,  ,"), CorsPolicy::Disabled);
        assert!(!CorsPolicy::Disabled.allows("https://example.com"));
    }

    #[test]
    fn a_bare_wildcard_is_not_accepted_as_an_allowlist() {
        // `*` in config would re-create the permissive behaviour this replaces,
        // so it is filtered out rather than honoured.
        assert_eq!(CorsPolicy::parse("*"), CorsPolicy::Disabled);
        assert_eq!(
            CorsPolicy::parse("*, https://ops.example.com"),
            CorsPolicy::Allowlist(vec!["https://ops.example.com".into()]),
            "the wildcard is dropped, the real origin is kept"
        );
    }

    #[test]
    fn allowlist_permits_only_listed_origins() {
        let policy = CorsPolicy::parse("https://ops.example.com, https://admin.example.com");
        assert!(policy.allows("https://ops.example.com"));
        assert!(policy.allows("https://admin.example.com"));

        assert!(!policy.allows("https://evil.example.com"));
        // Matching is exact: scheme, host and port all matter.
        assert!(!policy.allows("http://ops.example.com"));
        assert!(!policy.allows("https://ops.example.com:8443"));
        assert!(!policy.allows("https://ops.example.com.evil.com"));
    }

    #[test]
    fn allowlist_trims_surrounding_whitespace() {
        assert_eq!(
            CorsPolicy::parse("  https://a.example.com  ,\thttps://b.example.com\n"),
            CorsPolicy::Allowlist(vec![
                "https://a.example.com".into(),
                "https://b.example.com".into()
            ])
        );
    }

    #[test]
    fn every_policy_builds_a_layer() {
        // Guards against a panic in HeaderValue conversion for odd input.
        let _ = CorsPolicy::Disabled.layer();
        let _ = CorsPolicy::parse("https://ops.example.com").layer();
        let _ = CorsPolicy::parse("not a valid header value\u{7f}").layer();
    }

    #[test]
    fn default_limits_are_bounded_and_nonzero() {
        let limits = GatewayLimits::default();
        assert_eq!(limits.max_body_bytes, 1024 * 1024);
        assert_eq!(limits.max_concurrent_requests, 64);
        assert_eq!(limits.max_ws_message_bytes, 256 * 1024);
        assert!(limits.max_body_bytes > 0);
        assert!(limits.max_concurrent_requests > 0);
        assert!(limits.max_ws_message_bytes > 0);
    }

    /// A malformed or zero value must fall back to the default rather than
    /// becoming "reject everything", which would present as an outage rather
    /// than a misconfiguration.
    #[test]
    fn malformed_or_zero_limit_values_fall_back_to_defaults() {
        // Exercised through the same parse logic from_env uses.
        let read = |raw: Option<&str>, default: usize| -> usize {
            raw.and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(default)
        };
        assert_eq!(read(Some("abc"), 99), 99, "non-numeric falls back");
        assert_eq!(read(Some(""), 99), 99, "empty falls back");
        assert_eq!(read(Some("0"), 99), 99, "zero falls back, never disables");
        assert_eq!(read(Some("-5"), 99), 99, "negative falls back");
        assert_eq!(read(None, 99), 99, "unset falls back");
        assert_eq!(read(Some(" 512 "), 99), 512, "a real value is honoured");
    }
}
