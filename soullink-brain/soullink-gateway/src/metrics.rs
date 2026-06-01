//! Prometheus metrics definitions for the gateway.
//!
//! Uses the `metrics` crate (workspace dep v0.22) for metric registration
//! and `metrics-exporter-prometheus` with `http-listener` for serving.
//!
//! # Usage
//!
//! ```ignore
//! use metrics::counter;
//! counter!(crate::metrics::CHAT_REQUESTS).increment(1);
//! ```
//!
//! Serve on the configured metrics port:
//! ```ignore
//! use metrics_exporter_prometheus::PrometheusBuilder;
//! PrometheusBuilder::new()
//!     .with_http_listener(addr)
//!     .install()
//!     .expect("prometheus exporter install");
//! ```

use metrics::{describe_counter, describe_gauge, describe_histogram, Unit};

// ── Counters ──────────────────────────────────────────────────────────────

/// Total messages received from any channel.
pub const MESSAGES_RECEIVED: &str = "soullink_messages_received_total";
/// Total messages sent to any channel.
pub const MESSAGES_SENT: &str = "soullink_messages_sent_total";
/// Total chat API requests.
pub const CHAT_REQUESTS: &str = "soullink_chat_requests_total";
/// Total requests forwarded to an LLM provider.
pub const PROVIDER_REQUESTS: &str = "soullink_provider_requests_total";
/// Total provider request failures.
pub const PROVIDER_ERRORS: &str = "soullink_provider_errors_total";
/// Total WebSocket connections accepted.
pub const WEBSOCKET_CONNECTIONS: &str = "soullink_websocket_connections_total";
/// Total gateway lock acquisition attempts.
pub const LOCK_ATTEMPTS: &str = "soullink_lock_attempts_total";
/// Total gateway lock failures.
pub const LOCK_FAILURES: &str = "soullink_lock_failures_total";

// ── Histograms ────────────────────────────────────────────────────────────

/// Chat request latency (seconds).
pub const CHAT_LATENCY: &str = "soullink_chat_latency_seconds";
/// Provider round-trip latency (seconds).
pub const PROVIDER_LATENCY: &str = "soullink_provider_latency_seconds";

// ── Gauges ────────────────────────────────────────────────────────────────

/// Number of currently connected WebSocket clients.
pub const ACTIVE_CONNECTIONS: &str = "soullink_active_connections";
/// Number of pending/in-flight LLM requests.
pub const QUEUE_DEPTH: &str = "soullink_queue_depth";

/// Call once at startup to register metric descriptions.
pub fn register_descriptions() {
    describe_counter!(
        MESSAGES_RECEIVED,
        Unit::Count,
        "Total messages received from any messaging channel"
    );
    describe_counter!(
        MESSAGES_SENT,
        Unit::Count,
        "Total messages sent to any messaging channel"
    );
    describe_counter!(
        CHAT_REQUESTS,
        Unit::Count,
        "Total chat API requests received"
    );
    describe_counter!(
        PROVIDER_REQUESTS,
        Unit::Count,
        "Total requests forwarded to an LLM provider"
    );
    describe_counter!(
        PROVIDER_ERRORS,
        Unit::Count,
        "Total provider request failures"
    );
    describe_counter!(
        WEBSOCKET_CONNECTIONS,
        Unit::Count,
        "Total WebSocket connections accepted"
    );
    describe_counter!(
        LOCK_ATTEMPTS,
        Unit::Count,
        "Total gateway lock acquisition attempts"
    );
    describe_counter!(
        LOCK_FAILURES,
        Unit::Count,
        "Total gateway lock acquisition failures"
    );

    describe_histogram!(
        CHAT_LATENCY,
        Unit::Seconds,
        "Chat request latency distribution"
    );
    describe_histogram!(
        PROVIDER_LATENCY,
        Unit::Seconds,
        "Provider round-trip latency distribution"
    );

    describe_gauge!(
        ACTIVE_CONNECTIONS,
        Unit::Count,
        "Current number of connected WebSocket clients"
    );
    describe_gauge!(
        QUEUE_DEPTH,
        Unit::Count,
        "Current number of in-flight LLM requests"
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_metric_names_are_non_empty() {
        use super::*;
        assert!(!MESSAGES_RECEIVED.is_empty());
        assert!(!MESSAGES_SENT.is_empty());
        assert!(!CHAT_REQUESTS.is_empty());
        assert!(!PROVIDER_REQUESTS.is_empty());
        assert!(!PROVIDER_ERRORS.is_empty());
        assert!(!WEBSOCKET_CONNECTIONS.is_empty());
        assert!(!LOCK_ATTEMPTS.is_empty());
        assert!(!LOCK_FAILURES.is_empty());
        assert!(!CHAT_LATENCY.is_empty());
        assert!(!PROVIDER_LATENCY.is_empty());
        assert!(!ACTIVE_CONNECTIONS.is_empty());
        assert!(!QUEUE_DEPTH.is_empty());
    }
}
