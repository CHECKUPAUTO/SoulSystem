use avid_core::agents::base::AgentBase;
use avid_core::llm::{ChatBackend, LlmError};
use avid_mimic::{APISpec, MimicEngine};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
struct MockMimicBackend;

#[async_trait::async_trait]
impl ChatBackend for MockMimicBackend {
    async fn chat(&self, _s: &str, _u: &str, _j: bool, _t: Duration) -> Result<String, LlmError> {
        Ok(r#"{"language": "python", "files": {"main.py": "print('hello')"}, "entrypoint": "main.py"}"#.to_string())
    }
    fn model(&self) -> &str {
        "mock"
    }
}

#[tokio::test]
async fn test_mimic_clone_api() {
    let agent = AgentBase::new(Arc::new(MockMimicBackend));
    let engine = MimicEngine::new(agent);
    let spec = APISpec {
        base_url: "http://example.com".to_string(),
        endpoints: vec![],
    };
    let output = engine.clone_api(&spec).await.unwrap();
    assert!(output.entrypoint.contains("main.py"));
}

#[tokio::test]
async fn test_mimic_clone_logic() {
    let agent = AgentBase::new(Arc::new(MockMimicBackend));
    let engine = MimicEngine::new(agent);
    let output = engine.clone_logic("a simple calculator").await.unwrap();
    assert!(output.entrypoint.contains("main.py"));
}
