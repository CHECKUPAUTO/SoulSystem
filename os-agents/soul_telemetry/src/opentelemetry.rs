use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use std::sync::OnceLock;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, Registry};

static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

pub fn init_tracing() -> SdkTracerProvider {
    let tracer_provider = TRACER_PROVIDER.get_or_init(|| {
        let resource = Resource::builder_empty()
            .with_service_name("soul-system")
            .with_attribute(KeyValue::new(
                "service.version",
                env!("CARGO_PKG_VERSION"),
            ))
            .build();

        let provider = SdkTracerProvider::builder()
            .with_resource(resource)
            .build();
        global::set_tracer_provider(provider.clone());

        let subscriber = Registry::default()
            .with(LevelFilter::INFO)
            .with(tracing_subscriber::fmt::layer().json());
        let _ = tracing::subscriber::set_global_default(subscriber);

        provider
    });

    tracer_provider.clone()
}

pub fn shutdown_tracing() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        let _ = provider.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[test]
    fn test_tracing_initialization() {
        let _provider = init_tracing();
        info!("Test tracing initialization");
    }
}
