use crate::budget::LlmBudget;
use crate::error::{LlmError, Result};
use crate::factory::create_provider;
use crate::provider::LlmProvider;
use crate::retry::{RetryDecision, RetryPolicy, StopReason};
use crate::types::{GenerateRequest, LlmConfig, ModelInfo, StreamChunk};
use futures::stream::BoxStream;
use futures::Stream;
use std::future::Future;
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

    /// Run `op` under the configured [`RetryPolicy`] (MED-001).
    ///
    /// The concurrency permit is acquired by the caller and held across every
    /// attempt, including the backoff sleeps: the retries are one logical
    /// request, so they must occupy one in-flight slot rather than releasing it
    /// and re-queuing behind other callers mid-sequence.
    ///
    /// Budget accounting is likewise the caller's, and runs once — a retried
    /// request is charged for the tokens it actually consumed on the attempt
    /// that succeeded, not once per attempt.
    async fn with_retry<T, F, Fut>(&self, what: &str, op: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let policy = self.config.retry;
        let mut attempt: u32 = 1;
        loop {
            let error = match op().await {
                Ok(value) => return Ok(value),
                Err(e) => e,
            };

            match policy.decide(attempt, &error) {
                RetryDecision::After(delay) => {
                    tracing::warn!(
                        operation = what,
                        provider = self.provider.name(),
                        attempt,
                        max_attempts = policy.max_attempts,
                        delay_ms = delay.as_millis() as u64,
                        error = %error,
                        "provider call failed; retrying after backoff"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                RetryDecision::Stop(StopReason::NotRetryable) => return Err(error),
                RetryDecision::Stop(StopReason::ServerBackoffTooLong) => {
                    // Returned unwrapped, so the caller still sees the
                    // server's own interval and can decide to wait it out.
                    tracing::warn!(
                        operation = what,
                        provider = self.provider.name(),
                        error = %error,
                        "provider asked for a longer backoff than the policy allows; not retrying"
                    );
                    return Err(error);
                }
                RetryDecision::Stop(StopReason::AttemptsExhausted) => {
                    if attempt == 1 {
                        // Retrying is switched off entirely; wrapping a single
                        // failure as "exhausted" would misreport it.
                        return Err(error);
                    }
                    tracing::warn!(
                        operation = what,
                        provider = self.provider.name(),
                        attempts = attempt,
                        error = %error,
                        "provider call failed on every attempt"
                    );
                    return Err(LlmError::RetriesExhausted {
                        attempts: attempt,
                        last: Box::new(error),
                    });
                }
            }
        }
    }

    /// Accès au provider sous-jacent.
    pub fn provider(&self) -> &dyn LlmProvider {
        self.provider.as_ref()
    }

    /// The retry policy this client applies to provider calls.
    pub fn retry_policy(&self) -> RetryPolicy {
        self.config.retry
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
        let result = self
            .with_retry("generate", || self.provider.generate(&req))
            .await?;

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
        // Only *establishment* is retried. Once the stream exists a chunk may
        // already have been handed to the caller, and re-running the request
        // would duplicate output rather than recover from it — so a mid-stream
        // failure is the caller's to handle, not this loop's.
        let inner = self
            .with_retry("generate_stream", || {
                self.provider.generate_stream(request.clone())
            })
            .await?;
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
        self.with_retry("chat", || self.provider.chat(messages, tools))
            .await
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

    // ── MED-001 / MED-009: bounded retry with backoff ────────────────────

    /// A provider that fails the first `fail_first` calls with a scripted
    /// error, then succeeds. Records how many times it was actually called, so
    /// the tests assert on real dispatches rather than on the policy struct.
    struct FailingProvider {
        calls: AtomicUsize,
        fail_first: usize,
        error: Box<dyn Fn() -> LlmError + Send + Sync>,
    }

    impl FailingProvider {
        fn new(fail_first: usize, error: impl Fn() -> LlmError + Send + Sync + 'static) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail_first,
                error: Box::new(error),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmProvider for FailingProvider {
        fn name(&self) -> &str {
            "failing-mock"
        }
        async fn health_check(&self) -> bool {
            true
        }
        async fn list_models(&self) -> Result<Vec<ModelInfo>> {
            Ok(vec![])
        }
        async fn generate(&self, _request: &GenerateRequest) -> Result<GenerateResult> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n <= self.fail_first {
                return Err((self.error)());
            }
            Ok(GenerateResult {
                text: "recovered".into(),
                usage: TokenUsage::new(1, 1),
                model: "mock".into(),
                duration_ms: 0,
            })
        }
        async fn generate_stream(
            &self,
            _request: GenerateRequest,
        ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n <= self.fail_first {
                return Err((self.error)());
            }
            Ok(Box::pin(futures::stream::empty()))
        }
        async fn embed(&self, _text: &str, _model: Option<&str>) -> Result<EmbeddingResult> {
            unimplemented!("not exercised by the retry tests")
        }
    }

    fn fast_retry_config(max_attempts: u32) -> LlmConfig {
        LlmConfig {
            retry: crate::retry::RetryPolicy {
                max_attempts,
                // Keep the tests quick; the backoff *curve* is unit-tested in
                // `retry.rs`, what matters here is that the loop uses it.
                initial_backoff: Duration::from_millis(1),
                max_backoff: Duration::from_millis(20),
                multiplier: 2.0,
                jitter: false,
            },
            ..unlimited_budget_config(0)
        }
    }

    #[test]
    fn retry_is_on_by_default() {
        assert!(LlmConfig::default().retry.is_enabled());
    }

    #[tokio::test]
    async fn a_transient_failure_is_retried_and_the_call_succeeds() {
        let provider = Arc::new(FailingProvider::new(2, || {
            LlmError::Network("connection reset".into())
        }));
        let client = LlmClient::with_provider(provider.clone(), fast_retry_config(3));

        let result = client.generate("hello").await.expect("should recover");
        assert_eq!(result.text, "recovered");
        assert_eq!(
            provider.calls(),
            3,
            "two failures then a success is three real dispatches"
        );
    }

    #[tokio::test]
    async fn a_permanent_failure_is_not_retried_at_all() {
        let provider = Arc::new(FailingProvider::new(99, || {
            LlmError::Auth("invalid api key".into())
        }));
        let client = LlmClient::with_provider(provider.clone(), fast_retry_config(5));

        let err = client.generate("hello").await.unwrap_err();
        assert!(matches!(err, LlmError::Auth(_)), "got {err}");
        assert_eq!(
            provider.calls(),
            1,
            "a bad key is not fixed by asking four more times"
        );
    }

    #[tokio::test]
    async fn exhausting_the_budget_reports_retries_exhausted_not_the_bare_error() {
        // This is the cross-layer signal (MED-009): an outer replan/abort
        // decision must be able to tell "failed once" from "the provider layer
        // already backed off three times".
        let provider = Arc::new(FailingProvider::new(99, || LlmError::ServiceUnavailable {
            status: 503,
            retry_after: None,
        }));
        let client = LlmClient::with_provider(provider.clone(), fast_retry_config(3));

        let err = client.generate("hello").await.unwrap_err();
        let LlmError::RetriesExhausted { attempts, last } = err else {
            panic!("expected RetriesExhausted, got {err}");
        };
        assert_eq!(attempts, 3);
        assert!(matches!(*last, LlmError::ServiceUnavailable { .. }));
        assert_eq!(provider.calls(), 3);
        assert!(
            !LlmError::RetriesExhausted {
                attempts,
                last: Box::new(LlmError::Network("x".into()))
            }
            .is_retryable(),
            "an outer layer asking is_retryable must be told no"
        );
    }

    #[tokio::test]
    async fn a_single_attempt_policy_returns_the_bare_error_not_a_wrapper() {
        // With retrying switched off, "exhausted after 1 attempt" would
        // misreport an ordinary one-shot failure.
        let provider = Arc::new(FailingProvider::new(99, || {
            LlmError::Network("boom".into())
        }));
        let client = LlmClient::with_provider(provider.clone(), fast_retry_config(1));

        let err = client.generate("hello").await.unwrap_err();
        assert!(matches!(err, LlmError::Network(_)), "got {err}");
        assert_eq!(provider.calls(), 1);
    }

    #[tokio::test]
    async fn a_server_backoff_longer_than_the_ceiling_returns_the_rate_limit_intact() {
        // Not wrapped as RetriesExhausted: the caller needs the server's own
        // interval to decide whether to wait it out.
        let provider = Arc::new(FailingProvider::new(99, || LlmError::RateLimited {
            retry_after: Some(3600),
        }));
        let client = LlmClient::with_provider(provider.clone(), fast_retry_config(5));

        let err = client.generate("hello").await.unwrap_err();
        assert!(
            matches!(
                err,
                LlmError::RateLimited {
                    retry_after: Some(3600)
                }
            ),
            "got {err}"
        );
        assert_eq!(
            provider.calls(),
            1,
            "retrying early would just earn another 429"
        );
    }

    #[tokio::test]
    async fn the_retry_loop_actually_waits_between_attempts() {
        let provider = Arc::new(FailingProvider::new(2, || {
            LlmError::Network("reset".into())
        }));
        let config = LlmConfig {
            retry: crate::retry::RetryPolicy {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(40),
                max_backoff: Duration::from_secs(1),
                multiplier: 2.0,
                jitter: false,
            },
            ..unlimited_budget_config(0)
        };
        let client = LlmClient::with_provider(provider.clone(), config);

        let start = std::time::Instant::now();
        client.generate("hello").await.unwrap();
        let elapsed = start.elapsed();

        // 40ms after the first failure + 80ms after the second.
        assert!(
            elapsed >= Duration::from_millis(110),
            "expected to have slept through the backoff, took only {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn stream_establishment_is_retried() {
        // Only establishment — once a chunk may have reached the caller,
        // re-running the request would duplicate output rather than recover.
        let provider = Arc::new(FailingProvider::new(1, || {
            LlmError::Network("reset".into())
        }));
        let client = LlmClient::with_provider(provider.clone(), fast_retry_config(3));

        assert!(client.stream("hello").await.is_ok());
        assert_eq!(provider.calls(), 2);
    }

    #[tokio::test]
    async fn a_retried_request_is_charged_to_the_budget_once() {
        let provider = Arc::new(FailingProvider::new(2, || {
            LlmError::Network("reset".into())
        }));
        let config = LlmConfig {
            goal_token_budget: 1_000_000,
            ..fast_retry_config(3)
        };
        let client = LlmClient::with_provider(provider.clone(), config);

        client.generate_with_goal("hello", "g1").await.unwrap();

        let usage = client
            .budget()
            .get_goal_usage("g1")
            .expect("the successful attempt should have been recorded");
        assert_eq!(
            usage.total_tokens, 2,
            "three dispatches, one success — the failed attempts consumed no \
             tokens and must not be billed"
        );
    }
}
