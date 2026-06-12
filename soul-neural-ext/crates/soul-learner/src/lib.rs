use async_trait::async_trait;
use soul_core::Outcome;
use soul_cortex::Cortex;
use soul_identity::Identity;
use soul_tools::ToolOrchestrator;
use anyhow::Result;

#[async_trait]
pub trait Scenario: Send + Sync {
    fn name(&self) -> &str;
    async fn run_episode(&self, cortex: &mut Cortex, identity: &mut Identity, tools: &mut ToolOrchestrator) -> Result<f64>;
}

pub struct QAScenario {
    questions: Vec<(String, String)>,
}

impl QAScenario {
    pub fn new() -> Self {
        Self {
            questions: vec![
                ("Quelle est la capitale de la France ?".into(), "Paris".into()),
                ("2 + 2 ?".into(), "4".into()),
            ],
        }
    }
}

#[async_trait]
impl Scenario for QAScenario {
    fn name(&self) -> &str { "qa-basic" }

    async fn run_episode(&self, cortex: &mut Cortex, identity: &mut Identity, _tools: &mut ToolOrchestrator) -> Result<f64> {
        let mut total_reward = 0.0;
        for (question, expected) in &self.questions {
            cortex.perceive(question).await?;
            let answer = cortex.generate(question, 50).await?;
            let reward = if answer.trim().to_lowercase().contains(&expected.to_lowercase()) {
                1.0
            } else {
                -0.1
            };
            total_reward += reward;
            identity.record_episode(
                "agent", "answer", &answer, 0.5,
                Some(if reward > 0.0 { Outcome::Success(reward) } else { Outcome::Failure(reward) }),
            ).await?;
        }
        Ok(total_reward)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_cortex::Cortex;
    use soul_embed::Embedder;
    use soul_hnn::HamiltonianNN;
    use soul_memory_store::SharedVectorStore;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_qascenario_run() {
        let hnn = HamiltonianNN::new(128);
        let llm = soul_minillm::MiniLLM::new(&["hello", "test", "Paris"]);
        let embedder = Embedder::new(&["hello", "test"]);
        let cortex = Arc::new(Mutex::new(Cortex::new(hnn, llm, embedder.clone())));
        let store = SharedVectorStore::new(128, 10);
        let identity = Arc::new(Mutex::new(soul_identity::Identity::new(embedder, Arc::new(store))));
        let tools = Arc::new(Mutex::new(soul_tools::ToolOrchestrator::new(Embedder::new(&["test"]))));
        let scenario = QAScenario::new();
        let reward = scenario.run_episode(
            &mut *cortex.lock().unwrap(),
            &mut *identity.lock().unwrap(),
            &mut *tools.lock().unwrap(),
        ).await
        .expect("run_episode should not fail");
        assert!(reward.is_finite());
    }
}
