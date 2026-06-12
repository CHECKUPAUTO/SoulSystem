use crate::llm::LlmProvider;
use std::time::Duration;
use std::sync::Arc;

pub struct ReasoningChain {
    provider: Arc<dyn LlmProvider>,
    max_steps: usize,
}

impl ReasoningChain {
    pub fn new(provider: Arc<dyn LlmProvider>, max_steps: usize) -> Self {
        Self { provider, max_steps }
    }

    pub async fn solve(&self, initial_prompt: &str) -> Result<String, anyhow::Error> {
        let mut current_state = initial_prompt.to_string();
        for _ in 0..self.max_steps {
            let system = "You are a reasoning engine. Break down the task and solve it step by step.";
            let response = self.provider.complete(system, &current_state, Duration::from_secs(60)).await?;
            if response.contains("FINAL_ANSWER") { return Ok(response); }
            current_state = format!("Previous thoughts: {}\nNext step:", response);
        }
        Ok(current_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockChainProvider;
    #[async_trait]
    impl LlmProvider for MockChainProvider {
        async fn complete(&self, _s: &str, _u: &str, _t: Duration) -> Result<String, anyhow::Error> {
            Ok("FINAL_ANSWER: 42".to_string())
        }
        fn name(&self) -> &str { "mock" }
    }

    #[tokio::test]
    async fn test_reasoning_chain() {
        let chain = ReasoningChain::new(Arc::new(MockChainProvider), 3);
        let res = chain.solve("What is life?").await.unwrap();
        assert!(res.contains("42"));
    }

    #[test]
    fn test_chain_initialization() {
        let chain = ReasoningChain::new(Arc::new(MockChainProvider), 10);
        assert_eq!(chain.max_steps, 10);
    }
}
