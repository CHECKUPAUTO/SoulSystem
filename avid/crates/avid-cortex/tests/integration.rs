use avid_core::agents::base::AgentBase;
use avid_core::llm::{ChatBackend, LlmError};
use avid_cortex::CortexEngine;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
struct MockCortexBackend;

#[async_trait::async_trait]
impl ChatBackend for MockCortexBackend {
    async fn chat(&self, _s: &str, _u: &str, _j: bool, _t: Duration) -> Result<String, LlmError> {
        Ok(r#"{"title": "Test Paper", "abstract_text": "Content", "key_findings": [], "steps": [], "entities": []}"#.to_string())
    }
    fn model(&self) -> &str {
        "mock"
    }
}

#[tokio::test]
async fn test_cortex_workflow_extraction() {
    let agent = AgentBase::new(Arc::new(MockCortexBackend));
    let engine = CortexEngine::new(agent);
    let workflow = engine.extract_workflow("process").await.unwrap();
    assert!(workflow.steps.is_empty());
}
