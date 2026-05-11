use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: Uuid,
    pub topic: String,
    pub kind: IntentKind,
    pub payload: serde_json::Value,
    pub priority: u8,
    pub emitted_at: i64,
    pub correlation_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentKind {
    ClonePipeline,
    ScoutOnly,
    VisionAnalyze,
    CortexReadPaper,
    MimicCloneApi,
    AnticloneCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResult {
    pub intent_id: Uuid,
    pub topic: String,
    pub success: bool,
    pub started_at: i64,
    pub finished_at: i64,
    pub originality_score: Option<f32>,
    pub originality_verdict: Option<OriginalityVerdict>,
    pub stage_durations_ms: std::collections::HashMap<String, u64>,
    pub error: Option<String>,
    pub report: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OriginalityVerdict {
    Original,
    Clone,
}
