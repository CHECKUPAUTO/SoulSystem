// Pont soul-neural-bridge
#[cfg(feature = "std")]
pub fn init() -> anyhow::Result<()> {
    tracing::info!("Bridge {} active", stringify!(soul - neural - bridge));
    Ok(())
}
