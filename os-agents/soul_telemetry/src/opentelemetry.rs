use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{
    trace::TracerProvider,
    Resource,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, Registry};

pub fn init_tracing() -> TracerProvider {
    let resource = Resource::new(vec![
        KeyValue::new("service.name", "soul-system"),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    let tracer_provider = TracerProvider::builder()
        .with_resource(resource)
        .build();

    global::set_tracer_provider(tracer_provider.clone());

    let subscriber = Registry::default()
        .with(LevelFilter::INFO)
        .with(tracing_subscriber::fmt::layer().json());

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    tracer_provider
}

pub fn shutdown_tracing() {
    global::shutdown_tracer_provider();
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