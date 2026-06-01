/// Formal verification stub behind the `formal` feature flag.
#[cfg(feature = "formal")]
pub struct FormalVerifier;

#[cfg(feature = "formal")]
impl FormalVerifier {
    /// Create a new stub formal verifier.
    pub fn new() -> Self { Self }
}

#[cfg(feature = "formal")]
pub trait Verify {
    fn verify(&self, _candidate: &crate::self_modify::patch_generator::PatchCandidate) -> anyhow::Result<bool>;
}

#[cfg(feature = "formal")]
impl Verify for FormalVerifier {
    fn verify(&self, _candidate: &crate::self_modify::patch_generator::PatchCandidate) -> anyhow::Result<bool> {
        Ok(true)
    }
}
