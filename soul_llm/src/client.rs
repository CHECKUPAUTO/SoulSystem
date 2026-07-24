use crate::budget::LlmBudget;
use crate::error::{LlmError, Result};
use crate::factory::create_provider;
use crate::provider::LlmProvider;
use crate::types::{GenerateRequest, LlmConfig, ModelInfo, StreamChunk};
use futures::stream::BoxStream;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Client LLM unifié — wrapper autour de `dyn LlmProvider` + budget.
///
/// Fournit une API backward-compatible avec l'ancien `OllamaClient`
/// tout en supportant n'importe quel provider.
#[derive(Clone)]
pub struct LlmClient {
    provider: Arc<dyn LlmProvider>,
    budget: Arc<LlmBudget>,
    config: LlmConfig,
    /// Bounds the number of provider requests in flight at once, independent
    /// of token budgeting (MED-002) — `None` when
    /// `config.max_concurrent_requests == 0` (disabled).
    concurrency: Option<Arc<Semaphore>>,
}

/// Wraps a provider's stream so the concurrency permit acquired for the
/// dispatch that created it stays held for the stream's entire lifetime, not
/// just the initial call — a stream is "in flight" for as long as it's being
/// polled, not just at creation.
struct PermitGuardedStream {
    inner: BoxStream<'static, Result<StreamChunk>>,
    _permit: Option<OwnedSemaphorePermit>,
}

impl Stream for PermitGuardedStream {
    type Item = Result<StreamChunk>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl LlmClient {
    /// Crée un client à partir de la configuration.
    pub fn new(config: LlmConfig) -> Result<Self> {
        let provider = create_provider(&config)?;
        let budget = Arc::new(LlmBudget::new(config.clone()));
        let concurrency = Self::build_semaphore(&config);
        Ok(Self {
            provider,
            budget,
            config,
            concurrency,
        })
    }

    /// Crée un client avec un provider pré-construit.
    pub fn with_provider(provider: Arc<dyn LlmProvider>, config: LlmConfig) -> Self {
        let budget = Arc::new(LlmBudget::new(config.clone()));
        let concurrency = Self::build_semaphore(&config);
        Self {
            provider,
            budget,
            config,
            concurrency,
        }
    }

    fn build_semaphore(config: &LlmConfig) -> Option<Arc<Semaphore>> {
        (config.max_concurrent_requests > 0)
            .then(|| Arc::new(Semaphore::new(config.max_concurrent_requests)))
    }

    /// Acquire an in-flight-request permit, if a concurrency cap is
    /// configured. Held by the caller (dropped at end of scope for a
    /// one-shot call, or moved into a [`PermitGuardedStream`] for a
    /// streaming call) so the cap counts requests still being serviced, not
    /// just requests dispatched.
    async fn acquire_permit(&self) -> Result<Option<OwnedSemaphorePermit>> {
        match &self.concurrency {
            None => Ok(None),
            Some(sem) => {
                let permit = sem.clone().acquire_owned().await.map_err(|e| {
                    LlmError::Provider(format!("concurrency semaphore closed: {e}"))
                })?;
                Ok(Some(permit))
            }
        }
    }

    /// Accès au provider sous-jacent.
    pub fn provider(&self) -> &dyn LlmProvider {
        self.provider.as_ref()
    }

    /// Accès au budget.
    pub fn budget(&self) -> &LlmBudget {
        &self.budget
    }

    /// Accès à la config.
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// Vérifie que le provider est joignable.
    pub async fn is_alive(&self) -> bool {
        self.provider.health_check().await
    }

    /// Liste les modèles disponibles.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        self.provider.list_models().await
    }

    /// Génère une réponse (batch).
    pub async fn generate(&self, prompt: &str) -> Result<crate::types::GenerateResult> {
        self.generate_with_goal(prompt, "default").await
    }

    /// Génère avec suivi de budget pour un goal spécifique.
    pub async fn generate_with_goal(
        &self,
        prompt: &str,
        goal_id: &str,
    ) -> Result<crate::types::GenerateResult> {
        let estimated = LlmBudget::estimate_tokens(prompt, self.config.max_tokens);
        self.budget.check_budget(goal_id, estimated)?;

        let req = GenerateRequest::new(prompt)
            .with_model(self.config.model.clone())
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens);

        let _permit = self.acquire_permit().await?;
        let result = self.provider.generate(&req).await?;

        self.budget.record_usage(goal_id, &result.usage);

        Ok(result)
    }

    /// Génère une réponse et retourne uniquement le texte (compatibilité).
    pub async fn generate_text(&self, prompt: &str) -> Result<String> {
        Ok(self.generate(prompt).await?.text)
    }

    /// Génère avec suivi de budget et retourne uniquement le texte.
    pub async fn generate_text_with_goal(&self, prompt: &str, goal_id: &str) -> Result<String> {
        Ok(self.generate_with_goal(prompt, goal_id).await?.text)
    }

    /// Génération streaming.
    pub async fn generate_stream(
        &self,
        request: GenerateRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        let permit = self.acquire_permit().await?;
        let inner = self.provider.generate_stream(request).await?;
        Ok(Box::pin(PermitGuardedStream {
            inner,
            _permit: permit,
        }))
    }

    /// Génération streaming (raccourci).
    pub async fn stream(&self, prompt: &str) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        let req = GenerateRequest::new(prompt)
            .with_model(self.config.model.clone())
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens)
            .with_stream(true);

        self.generate_stream(req).await
    }

    /// Chat completion with tool calling support.
    ///
    /// Routes to the provider's native chat API (Ollama `/api/chat`, etc.).
    /// Falls back to prompt completion for providers that don't support tools.
    pub async fn chat(
        &self,
        messages: &[crate::provider::ChatMessage],
        tools: Option<&[crate::provider::ToolSchema]>,
    ) -> Result<crate::provider::ChatResponse> {
        let _permit = self.acquire_permit().await?;
        self.provider.chat(messages, tools).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EmbeddingResult, GenerateResult, TokenUsage};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A provider that records how many `generate` calls are simultaneously
    /// in flight, tracking the observed peak. Each call holds the "in
    /// flight" slot for `delay` before completing, so concurrent callers
    /// actually overlap in time rather than racing through instantly.
    #[derive(Default)]
    struct ConcurrencyTrackingProvider {
        in_flight: AtomicUsize,
        peak: AtomicUsize,
        delay: Duration,
    }

    impl ConcurrencyTrackingProvider {
        fn with_delay(delay: Duration) -> Self {
            Self {
                delay,
                ..Default::default()
            }
        }
    }

    #[async_trait]
    impl LlmProvider for ConcurrencyTrackingProvider {
        fn name(&self) -> &str {
            "concurrency-tracking-mock"
        }

        async fn health_check(&self) -> bool {
            true
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>> {
            Ok(vec![])
        }

        async fn generate(&self, _request: &GenerateRequest) -> Result<GenerateResult> {
            let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(cur, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(GenerateResult {
                text: "ok".into(),
                usage: TokenUsage::new(1, 1),
                model: "mock".into(),
                duration_ms: 0,
            })
        }

        async fn generate_stream(
            &self,
            _request: GenerateRequest,
        ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
            unimplemented!("not exercised by the concurrency tests")
        }

        async fn embed(&self, _text: &str, _model: Option<&str>) -> Result<EmbeddingResult> {
            unimplemented!("not exercised by the concurrency tests")
        }
    }

    /// A provider whose `generate_stream` never completes on its own — used
    /// to prove a stream's concurrency permit is held for as long as the
    /// caller keeps the stream alive, independent of whether it's polled.
    #[derive(Default)]
    struct PendingStreamProvider;

    #[async_trait]
    impl LlmProvider for PendingStreamProvider {
        fn name(&self) -> &str {
            "pending-stream-mock"
        }
        async fn health_check(&self) -> bool {
            true
        }
        async fn list_models(&self) -> Result<Vec<ModelInfo>> {
            Ok(vec![])
        }
        async fn generate(&self, _request: &GenerateRequest) -> Result<GenerateResult> {
            unimplemented!("not exercised by the streaming test")
        }
        async fn generate_stream(
            &self,
            _request: GenerateRequest,
        ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
            Ok(Box::pin(futures::stream::pending()))
        }
        async fn embed(&self, _text: &str, _model: Option<&str>) -> Result<EmbeddingResult> {
            unimplemented!("not exercised by the streaming test")
        }
    }

    fn unlimited_budget_config(max_concurrent_requests: usize) -> LlmConfig {
        LlmConfig {
            max_concurrent_requests,
            goal_token_budget: 0,
            tokens_per_minute_budget: 0,
            ..Default::default()
        }
    }

    #[test]
    fn default_config_has_a_safe_nonzero_concurrency_cap() {
        assert!(LlmConfig::default().max_concurrent_requests > 0);
    }

    #[tokio::test]
    async fn concurrency_cap_bounds_peak_in_flight_generate_calls() {
        let provider = Arc::new(ConcurrencyTrackingProvider::with_delay(
            Duration::from_millis(30),
        ));
        let client = LlmClient::with_provider(provider.clone(), unlimited_budget_config(4));

        let futures = (0..20).map(|i| {
            let client = client.clone();
            async move { client.generate(&format!("prompt {i}")).await }
        });
        let results = futures::future::join_all(futures).await;
        assert!(results.iter().all(|r| r.is_ok()));

        assert!(
            provider.peak.load(Ordering::SeqCst) <= 4,
            "peak in-flight ({}) must never exceed max_concurrent_requests (4)",
            provider.peak.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn concurrency_cap_holds_independent_of_unlimited_token_budget() {
        // unlimited_budget_config already zeroes both token budgets — this
        // test exists to make that independence explicit and load-bearing:
        // if a future change coupled the concurrency gate to check_budget,
        // this would start failing even though the config above wouldn't.
        let provider = Arc::new(ConcurrencyTrackingProvider::with_delay(
            Duration::from_millis(30),
        ));
        let config = unlimited_budget_config(3);
        assert_eq!(config.goal_token_budget, 0);
        assert_eq!(config.tokens_per_minute_budget, 0);
        let client = LlmClient::with_provider(provider.clone(), config);

        let futures = (0..15).map(|_| {
            let client = client.clone();
            async move { client.generate("x").await }
        });
        futures::future::join_all(futures).await;

        assert!(provider.peak.load(Ordering::SeqCst) <= 3);
    }

    #[tokio::test]
    async fn zero_max_concurrent_requests_disables_the_cap() {
        let provider = Arc::new(ConcurrencyTrackingProvider::with_delay(
            Duration::from_millis(30),
        ));
        let client = LlmClient::with_provider(provider.clone(), unlimited_budget_config(0));

        let futures = (0..10).map(|_| {
            let client = client.clone();
            async move { client.generate("x").await }
        });
        futures::future::join_all(futures).await;

        assert_eq!(
            provider.peak.load(Ordering::SeqCst),
            10,
            "with the cap disabled all 10 calls must run fully concurrently"
        );
    }

    #[tokio::test]
    async fn streaming_permit_is_held_for_the_streams_lifetime_not_just_dispatch() {
        let provider = Arc::new(PendingStreamProvider);
        let client = LlmClient::with_provider(provider.clone(), unlimited_budget_config(1));

        let stream_a = client.stream("a").await.unwrap();

        // The only permit is held by stream_a (never polled/drained, but
        // still alive) — a second stream must not be able to acquire one.
        let second = tokio::time::timeout(Duration::from_millis(50), client.stream("b")).await;
        assert!(
            second.is_err(),
            "a second stream must block while the first still holds the only permit"
        );

        drop(stream_a);

        let third = tokio::time::timeout(Duration::from_millis(200), client.stream("c")).await;
        assert!(
            third.is_ok(),
            "dropping the first stream must release its permit for the next caller"
        );
    }
}
