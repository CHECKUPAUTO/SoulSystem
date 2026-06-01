/// Pont AVID ↔ SoulSystem
/// Re-exporte les composants AVID pour usage dans SoulSystem

#[cfg(feature = "avid")]
pub use avid_scout as scout;
#[cfg(feature = "avid")]
pub use avid_vision as vision;
#[cfg(feature = "avid")]
pub use avid_core as core;
#[cfg(feature = "avid")]
pub use avid_cortex as cortex;

/// Fonction utilitaire : initialise une session AVID
#[cfg(feature = "avid")]
pub fn init() -> anyhow::Result<()> {
    tracing::info!("🧬 AVID bridge initialized");
    Ok(())
}

#[cfg(not(feature = "avid"))]
pub fn init() -> anyhow::Result<()> {
    tracing::warn!("⚠️ AVID bridge not available (compile with --features avid)");
    Ok(())
}