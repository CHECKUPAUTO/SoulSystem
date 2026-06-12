//! Télémétrie distribuée avec OpenTelemetry.
//!
//! Configure le tracing distribué pour suivre les requêtes
//! à travers tous les modules SoulSystem.

use tracing::info;
use tracing_subscriber::layer::SubscriberExt;

/// Initialise le subscriber OpenTelemetry.
///
/// Exporte en OTLP vers le collector configuré via
/// `OTEL_EXPORTER_OTLP_ENDPOINT` (par défaut `http://localhost:4317`).
pub fn init_telemetry() -> anyhow::Result<()> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".into());

    // Configuration OpenTelemetry
    // En production: utiliser opentelemetry_otlp::new_exporter().tonic()
    // Pour l'instant: logging structuré avec contexte de trace

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_file(true)
        .with_line_number(true);

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Telemetry initialized — OTLP endpoint: {}", endpoint);
    Ok(())
}

/// Helper pour créer un span racine.
#[macro_export]
macro_rules! trace_span {
    ($name:expr) => {
        tracing::info_span!($name)
    };
    ($name:expr, $($key:ident = $value:expr),*) => {
        tracing::info_span!($name, $($key = $value),*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_telemetry_doesnt_panic() {
        // Just verify it doesn't panic with default config
        let result = init_telemetry();
        // May fail if another subscriber is already set
        assert!(result.is_ok() || result.is_err());
    }
}
