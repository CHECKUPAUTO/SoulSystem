use opentelemetry::global;
use opentelemetry::KeyValue;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, Registry};

/// Build the SDK tracer provider (resource + provider), install it as the
/// process-global provider, and return it so the caller can deterministically
/// shut it down. This does not touch the global `tracing` subscriber.
pub fn init_tracer_provider() -> SdkTracerProvider {
    let resource = Resource::builder()
        .with_attributes([
            KeyValue::new("service.name", "soul-system"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ])
        .build();

    let provider = SdkTracerProvider::builder().with_resource(resource).build();

    global::set_tracer_provider(provider.clone());
    provider
}

/// Install the global `tracing` subscriber at most once for the process.
///
/// `set_global_default` can only succeed a single time; calling it again (for
/// example from a second test running concurrently) returns an error. We
/// deliberately ignore that error so repeated or parallel initialization is a
/// no-op instead of a panic.
pub fn init_tracing_subscriber() {
    let subscriber = Registry::default()
        .with(LevelFilter::INFO)
        .with(tracing_subscriber::fmt::layer().json());
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Convenience initializer that preserves the previous public entry point:
/// install the global subscriber (idempotent) and return the global tracer
/// provider guard for deterministic shutdown.
pub fn init_tracing() -> SdkTracerProvider {
    init_tracing_subscriber();
    init_tracer_provider()
}

/// Deterministically shut down a tracer provider (flush + stop its span
/// processors). Replaces the `global::shutdown_tracer_provider()` helper that
/// was removed in OpenTelemetry 0.31. A shutdown error (e.g. the provider was
/// already shut down) is ignored so callers never panic on teardown.
pub fn shutdown_tracing(provider: &SdkTracerProvider) {
    let _ = provider.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[test]
    fn test_tracing_initialization() {
        // init_tracing installs the global subscriber idempotently, so this is
        // safe even when other tests in the crate run concurrently.
        let provider = init_tracing();
        info!("Test tracing initialization");
        shutdown_tracing(&provider);
    }

    #[test]
    fn provider_init_and_shutdown_are_deterministic() {
        // The tracer provider can be built and shut down without touching the
        // global subscriber, so this test cannot conflict with others.
        let provider = init_tracer_provider();
        shutdown_tracing(&provider);
        // Shutting down twice must remain a no-op, not a panic.
        shutdown_tracing(&provider);
    }

    #[test]
    fn subscriber_init_is_idempotent() {
        // Installing the subscriber more than once must not panic.
        init_tracing_subscriber();
        init_tracing_subscriber();
    }
}
