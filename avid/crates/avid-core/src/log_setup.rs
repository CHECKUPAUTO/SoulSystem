use tracing_subscriber::fmt::layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Initialize tracing with JSON format to stdout.
pub fn setup_logging(log_level: &str) {
    let env_filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = layer()
        .json()
        .with_writer(std::io::stdout)
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}
