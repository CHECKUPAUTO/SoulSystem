use thiserror::Error;

/// Core errors for the AVID pipeline.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("queue error: {0}")]
    Queue(#[from] crate::queue::QueueError),

    #[error("memory error: {0}")]
    Memory(#[from] crate::memory::MemoryError),

    #[error("LLM error: {0}")]
    Llm(#[from] crate::llm::LlmError),

    #[error("agent error: {0}")]
    Agent(String),

    #[error("orchestrator error: {0}")]
    Orchestrator(String),

    #[error("anti-clone error: {0}")]
    AntiClone(#[from] avid_anticlone::AntiCloneError),

    #[error("sandbox error: {0}")]
    Sandbox(#[from] avid_sandbox::SandboxError),

    #[error("task timeout")]
    Timeout,
}
