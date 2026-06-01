// Pont synergie-bridge
#[cfg(feature = "std")]
pub fn init() -> anyhow::Result<()> {
    tracing::info!("Bridge {} active", stringify!(synergie-bridge));
    Ok(())
}
