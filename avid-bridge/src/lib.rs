/// Pont AVID ↔ SoulSystem
/// Les vrais types AVID sont disponibles via le crate avid-bridge avec la feature `avid`.
/// Sans la feature, des stubs permettent la compilation conditionnelle.

#[cfg(feature = "avid")]
compile_error!("La feature `avid` nécessite le workspace AVID séparé (build avec --manifest-path avid-bridge/Cargo.toml --features avid)");

pub fn init() -> anyhow::Result<()> {
    tracing::warn!("⚠️ AVID bridge: feature not enabled, using stubs");
    Ok(())
}

/// Stub Scout — remplacé par avid_scout::Scout quand la feature est activée
pub struct Scout;
impl Scout {
    pub async fn crawl(&self, _url: &str) -> anyhow::Result<()> {
        Ok(())
    }
}
impl Default for Scout {
    fn default() -> Self { Self }
}

/// Stub Vision — remplacé par avid_vision::Vision quand la feature est activée
pub struct Vision;
impl Default for Vision {
    fn default() -> Self { Self }
}