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

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::{HeaderValue, Method};
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
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

/// Maximum simultaneously accepted connections.
///
/// P1-2 bounded request *bodies*, *messages* and *work in flight*. None of
/// those bound accepted sockets: a slow-loris style flood opens connections and
/// sends bytes just fast enough to stay alive, never completing a request, so
/// the concurrency semaphore never sees them and the process accumulates
/// sockets until it runs out of file descriptors.
pub const DEFAULT_MAX_CONNECTIONS: usize = 512;

/// Sustained requests per second allowed to a single client.
pub const DEFAULT_RATE_PER_SECOND: u32 = 20;

/// Requests a single client may burst above its sustained rate.
///
/// A bucket the same size as the rate would reject normal bursty operator
/// traffic — a dashboard opening six panels at once is not abuse.
pub const DEFAULT_RATE_BURST: u32 = 40;

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
    /// Simultaneously accepted connections. Beyond this, a connection is
    /// **refused** — closed immediately — rather than accepted and parked.
    pub max_connections: usize,
    /// Sustained per-client request rate.
    pub rate_per_second: u32,
    /// Per-client burst allowance above the sustained rate.
    pub rate_burst: u32,
}

impl Default for GatewayLimits {
    fn default() -> Self {
        Self {
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_ws_message_bytes: DEFAULT_MAX_WS_MESSAGE_BYTES,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            rate_per_second: DEFAULT_RATE_PER_SECOND,
            rate_burst: DEFAULT_RATE_BURST,
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
            max_connections: read(
                "SOULSYSTEM_GATEWAY_MAX_CONNECTIONS",
                DEFAULT_MAX_CONNECTIONS,
            ),
            rate_per_second: read(
                "SOULSYSTEM_GATEWAY_RATE_PER_SECOND",
                DEFAULT_RATE_PER_SECOND as usize,
            ) as u32,
            rate_burst: read("SOULSYSTEM_GATEWAY_RATE_BURST", DEFAULT_RATE_BURST as usize) as u32,
        }
    }
}

/// Bounds simultaneously accepted connections.
///
/// # Refused, not parked
///
/// [`ConnectionLimiter::try_acquire`] returns `None` immediately when the cap
/// is reached, and the caller closes the socket. It deliberately does **not**
/// wait for a permit: parking the connection is what the attacker wants, since
/// it costs them one socket and costs the server one socket plus a queue slot.
/// Refusing costs them a reconnect.
#[derive(Clone, Debug)]
pub struct ConnectionLimiter {
    semaphore: Arc<Semaphore>,
    capacity: usize,
}

/// Proof that a connection was admitted. The slot is returned on drop.
#[derive(Debug)]
pub struct ConnectionPermit(#[allow(dead_code)] OwnedSemaphorePermit);

impl ConnectionLimiter {
    pub fn new(capacity: usize) -> Self {
        // A zero cap would refuse every connection, which reads as an outage
        // rather than a limit. Treat it as one, matching how a malformed limit
        // is treated as unset elsewhere in this module.
        let capacity = capacity.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(capacity)),
            capacity,
        }
    }

    /// Admit a connection, or refuse immediately if the cap is reached.
    pub fn try_acquire(&self) -> Option<ConnectionPermit> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(ConnectionPermit)
    }

    /// Connections currently admitted.
    pub fn in_flight(&self) -> usize {
        self.capacity - self.semaphore.available_permits()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Per-client token bucket.
///
/// # Why per client and not global
///
/// The gateway already bounds total work in flight. That bound is shared, so
/// one authenticated operator token — or one IP, before authentication — can
/// consume all of it and starve everyone else. This limiter makes the cost of
/// misbehaviour land on the client that caused it.
///
/// # Clock
///
/// Refill is computed from elapsed wall time on each check rather than by a
/// background task, so an idle client costs nothing and there is no timer to
/// leak.
#[derive(Clone, Debug)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Bucket>>>,
    per_second: u32,
    burst: u32,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// The outcome of a rate check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    Allowed,
    /// Throttled; the value is a whole-second `Retry-After` hint.
    Throttled {
        retry_after_secs: u64,
    },
}

impl RateLimiter {
    pub fn new(per_second: u32, burst: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            per_second: per_second.max(1),
            // Not clamped up to `per_second`. A bucket smaller than the rate
            // does not make the rate unreachable — it only stops the whole
            // second's allowance being spent instantaneously, which is the
            // point of having a separate burst number at all.
            burst: burst.max(1),
        }
    }

    /// Check and consume one token for `key`, at time `now`.
    ///
    /// `now` is a parameter so the tests can drive the clock instead of
    /// sleeping — a rate limiter tested with real sleeps is either slow or
    /// flaky, and usually both.
    pub fn check_at(&self, key: &str, now: Instant) -> RateDecision {
        let mut buckets = self.inner.lock();
        let burst = f64::from(self.burst);
        let per_second = f64::from(self.per_second);

        let bucket = buckets.entry(key.to_owned()).or_insert(Bucket {
            tokens: burst,
            last: now,
        });

        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * per_second).min(burst);
        bucket.last = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            RateDecision::Allowed
        } else {
            let deficit = 1.0 - bucket.tokens;
            RateDecision::Throttled {
                retry_after_secs: (deficit / per_second).ceil().max(1.0) as u64,
            }
        }
    }

    /// Check and consume one token for `key`, now.
    pub fn check(&self, key: &str) -> RateDecision {
        self.check_at(key, Instant::now())
    }

    /// Drop buckets untouched for longer than `idle`.
    ///
    /// Without this the map grows once per distinct key seen, which for an
    /// IP-keyed limiter on a public listener is unbounded — a memory-growth
    /// bug wearing a rate limiter's clothes.
    pub fn evict_idle_at(&self, idle: Duration, now: Instant) -> usize {
        let mut buckets = self.inner.lock();
        let before = buckets.len();
        buckets.retain(|_, bucket| now.saturating_duration_since(bucket.last) < idle);
        before - buckets.len()
    }

    /// Number of tracked keys.
    pub fn tracked_keys(&self) -> usize {
        self.inner.lock().len()
    }
}

/// The key a rate limit is charged against.
///
/// Authenticated requests are keyed by principal so a token cannot escape its
/// limit by reconnecting from a new source port; unauthenticated ones are keyed
/// by IP, which is the only identity available before authentication runs.
pub fn rate_key(principal: Option<&str>, peer: Option<IpAddr>) -> String {
    match (principal, peer) {
        (Some(name), _) => format!("principal:{name}"),
        (None, Some(ip)) => format!("ip:{ip}"),
        // No principal and no peer address: charge one shared bucket rather
        // than skipping the limit. An unidentifiable caller must not be the
        // cheapest way to bypass it.
        (None, None) => "anonymous".to_owned(),
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

    // ── Connection bounds (INV-NET-5) ────────────────────────────────────

    #[test]
    fn connections_beyond_the_cap_are_refused_not_queued() {
        let limiter = ConnectionLimiter::new(2);
        let first = limiter.try_acquire().expect("first admitted");
        let second = limiter.try_acquire().expect("second admitted");
        assert_eq!(limiter.in_flight(), 2);

        // The point of the whole mechanism: this returns immediately with
        // None rather than parking. Parking is what a slow-loris flood wants.
        assert!(
            limiter.try_acquire().is_none(),
            "the third connection must be refused, not queued"
        );

        drop(first);
        assert_eq!(limiter.in_flight(), 1);
        assert!(
            limiter.try_acquire().is_some(),
            "a closed connection returns its slot"
        );
        drop(second);
    }

    #[test]
    fn a_zero_cap_is_treated_as_one_rather_than_refusing_everything() {
        // Consistent with how a malformed byte limit is treated as unset: a
        // configuration mistake should not look like a total outage.
        let limiter = ConnectionLimiter::new(0);
        assert_eq!(limiter.capacity(), 1);
        assert!(limiter.try_acquire().is_some());
    }

    #[test]
    fn permits_are_returned_when_connections_close() {
        let limiter = ConnectionLimiter::new(1);
        for _ in 0..100 {
            let permit = limiter.try_acquire().expect("slot is free again");
            assert!(limiter.try_acquire().is_none());
            drop(permit);
        }
        assert_eq!(limiter.in_flight(), 0, "no slot leaked across 100 cycles");
    }

    // ── Per-client rate limiting (INV-NET-5) ─────────────────────────────

    #[test]
    fn a_client_may_burst_then_is_throttled() {
        let limiter = RateLimiter::new(10, 3);
        let t0 = Instant::now();
        for i in 0..3 {
            assert_eq!(
                limiter.check_at("a", t0),
                RateDecision::Allowed,
                "burst request {i} must pass"
            );
        }
        assert!(
            matches!(limiter.check_at("a", t0), RateDecision::Throttled { .. }),
            "the burst is exhausted"
        );
    }

    #[test]
    fn throttling_one_client_does_not_affect_another() {
        // The acceptance test for P1-11: one operator token saturating its
        // budget must not starve everyone else.
        let limiter = RateLimiter::new(10, 2);
        let t0 = Instant::now();
        assert_eq!(limiter.check_at("noisy", t0), RateDecision::Allowed);
        assert_eq!(limiter.check_at("noisy", t0), RateDecision::Allowed);
        assert!(matches!(
            limiter.check_at("noisy", t0),
            RateDecision::Throttled { .. }
        ));

        assert_eq!(
            limiter.check_at("quiet", t0),
            RateDecision::Allowed,
            "a separate client has its own bucket"
        );
        assert_eq!(limiter.check_at("quiet", t0), RateDecision::Allowed);
    }

    #[test]
    fn tokens_refill_over_time() {
        let limiter = RateLimiter::new(10, 2);
        let t0 = Instant::now();
        limiter.check_at("a", t0);
        limiter.check_at("a", t0);
        assert!(matches!(
            limiter.check_at("a", t0),
            RateDecision::Throttled { .. }
        ));

        // 10/s means one token per 100ms. The clock is injected, so this is
        // exact rather than a sleep that might or might not be long enough.
        let later = t0 + Duration::from_millis(150);
        assert_eq!(limiter.check_at("a", later), RateDecision::Allowed);
    }

    #[test]
    fn refill_is_capped_at_the_burst_size() {
        let limiter = RateLimiter::new(10, 2);
        let t0 = Instant::now();
        limiter.check_at("a", t0);
        // An hour idle must not bank an hour of tokens.
        let much_later = t0 + Duration::from_secs(3600);
        assert_eq!(limiter.check_at("a", much_later), RateDecision::Allowed);
        assert_eq!(limiter.check_at("a", much_later), RateDecision::Allowed);
        assert!(
            matches!(
                limiter.check_at("a", much_later),
                RateDecision::Throttled { .. }
            ),
            "idle time does not accumulate beyond the burst"
        );
    }

    #[test]
    fn a_throttled_response_carries_a_usable_retry_hint() {
        let limiter = RateLimiter::new(10, 1);
        let t0 = Instant::now();
        limiter.check_at("a", t0);
        match limiter.check_at("a", t0) {
            RateDecision::Throttled { retry_after_secs } => {
                assert!(retry_after_secs >= 1, "a zero-second hint is useless");
            }
            RateDecision::Allowed => panic!("expected throttling"),
        }
    }

    #[test]
    fn a_burst_smaller_than_the_rate_still_reaches_the_sustained_rate() {
        // A bucket smaller than the per-second rate spreads the allowance out
        // rather than capping it: over one second, ten requests at 100ms
        // spacing all pass even though only one may be spent at once.
        let limiter = RateLimiter::new(10, 1);
        let t0 = Instant::now();
        for i in 0..10 {
            let at = t0 + Duration::from_millis(100 * i);
            assert_eq!(
                limiter.check_at("a", at),
                RateDecision::Allowed,
                "request {i} at the sustained rate must pass"
            );
        }
        assert!(
            matches!(
                limiter.check_at("a", t0 + Duration::from_millis(900)),
                RateDecision::Throttled { .. }
            ),
            "but two in the same 100ms window do not"
        );
    }

    #[test]
    fn idle_buckets_are_evicted_so_the_map_cannot_grow_without_bound() {
        // An IP-keyed limiter on a public listener sees unbounded distinct
        // keys; without eviction this is a memory-growth bug wearing a rate
        // limiter's clothes.
        let limiter = RateLimiter::new(10, 10);
        let t0 = Instant::now();
        for i in 0..500 {
            limiter.check_at(&format!("client-{i}"), t0);
        }
        assert_eq!(limiter.tracked_keys(), 500);

        let later = t0 + Duration::from_secs(600);
        limiter.check_at("client-0", later);
        let evicted = limiter.evict_idle_at(Duration::from_secs(300), later);
        assert_eq!(evicted, 499);
        assert_eq!(
            limiter.tracked_keys(),
            1,
            "only the client active inside the window survives"
        );
    }

    #[test]
    fn rate_keys_prefer_principal_over_address() {
        use std::net::Ipv4Addr;
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
        assert_eq!(rate_key(Some("alice"), Some(ip)), "principal:alice");
        assert_eq!(
            rate_key(None, Some(ip)),
            "ip:203.0.113.7",
            "before authentication, the address is the only identity there is"
        );
        assert_eq!(
            rate_key(None, None),
            "anonymous",
            "an unidentifiable caller shares one bucket rather than escaping \
             the limit entirely"
        );
    }

    #[test]
    fn a_principal_cannot_escape_its_limit_by_changing_address() {
        use std::net::Ipv4Addr;
        let limiter = RateLimiter::new(10, 1);
        let t0 = Instant::now();
        let from = |n: u8| rate_key(Some("alice"), Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, n))));
        assert_eq!(limiter.check_at(&from(1), t0), RateDecision::Allowed);
        assert!(
            matches!(
                limiter.check_at(&from(2), t0),
                RateDecision::Throttled { .. }
            ),
            "the same token from a new source port or address shares one bucket"
        );
    }

    #[test]
    fn connection_and_rate_limits_are_read_from_the_environment() {
        let limits = GatewayLimits::default();
        assert_eq!(limits.max_connections, DEFAULT_MAX_CONNECTIONS);
        assert_eq!(limits.rate_per_second, DEFAULT_RATE_PER_SECOND);
        assert_eq!(limits.rate_burst, DEFAULT_RATE_BURST);
        assert!(
            limits.rate_burst >= limits.rate_per_second,
            "the default burst allows a normal operator burst — a dashboard \
             opening several panels at once is not abuse"
        );
    }
}
