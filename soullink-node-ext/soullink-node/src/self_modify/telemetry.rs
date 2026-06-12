use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Mutex;

/// Telemetry event emitted during evolution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EvolutionEvent {
    CycleStarted { timestamp: DateTime<Utc>, workspace_hash: String },
    NoCandidateFound,
    CandidateGenerated { id: uuid::Uuid, strategy: crate::self_modify::mutation_strategies::MutationStrategy },
    ConstitutionalViolation { id: uuid::Uuid, rule: String },
    ValidationFailed { id: uuid::Uuid, stage: String, reason: String },
    Applied { id: uuid::Uuid, score: f32 },
    RolledBack { id: uuid::Uuid, reason: String },
}

/// Simple in-memory telemetry buffer.
pub struct Telemetry {
    buffer: Mutex<Vec<EvolutionEvent>>,
}

impl Telemetry {
    /// Create a new telemetry instance.
    pub fn new() -> Self {
        Self { buffer: Mutex::new(Vec::new()) }
    }

    /// Emit an evolution event.
    pub fn emit(&self, event: EvolutionEvent) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.push(event);
        }
    }

    /// Return all buffered events.
    pub fn buffered_events(&self) -> Vec<EvolutionEvent> {
        self.buffer.lock().unwrap().clone()
    }
}
