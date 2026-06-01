// Pont openevolve-bridge
#[cfg(feature = "std")]
pub fn init() -> anyhow::Result<()> {
    tracing::info!("Bridge {} active", stringify!(openevolve-bridge));
    Ok(())
}
