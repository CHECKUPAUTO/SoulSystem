/// SoulSystem Mesh — Initialise et connecte tous les composants
/// Utilisation: cargo run --features all
// Unified bridge aliases (replaces 9 individual bridge crates)

#[cfg(feature = "avid")]
use soul_bridge::avid as avid;
#[cfg(feature = "brain")]
use soul_bridge::brain as brain;
#[cfg(feature = "openevolve")]
use soul_bridge::openevolve as openevolve;
#[cfg(feature = "soul_neural")]
use soul_bridge::soul_neural as soul_neural;
#[cfg(feature = "synergie")]
use soul_bridge::synergie as synergie;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    tracing::info!("🧬 SoulSystem Mesh initializing...");

    #[cfg(feature = "avid")]
    avid::init().ok();
    #[cfg(feature = "synergie")]
    synergie::init().ok();
    #[cfg(feature = "openevolve")]
    openevolve::init().ok();
    #[cfg(feature = "brain")]
    brain::init().ok();
    #[cfg(feature = "soul_neural")]
    soul_neural::init().ok();

    tracing::info!("✅ SoulSystem Mesh ready");
}
