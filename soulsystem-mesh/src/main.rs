/// SoulSystem Mesh — Initialise et connecte tous les composants
/// Utilisation: cargo run --features all

#[cfg(feature = "avid")]
use avid_bridge as avid;
#[cfg(feature = "brain")]
use brain_bridge as brain;
#[cfg(feature = "openevolve")]
use openevolve_bridge as openevolve;
#[cfg(feature = "soul_neural")]
use soul_neural_bridge as soul_neural;
#[cfg(feature = "synergie")]
use synergie_bridge as synergie;

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
