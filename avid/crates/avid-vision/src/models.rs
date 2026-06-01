use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use std::path::Path;

/// Manages ONNX model sessions.
pub struct ModelManager {}

impl ModelManager {
    pub fn new() -> Self { Self {} }

    /// Load a model from file.
    pub fn load_model<P: AsRef<Path>>(&self, path: P) -> Result<Session, anyhow::Error> {
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("{:?}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?
            .commit_from_file(path)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        Ok(session)
    }
}
