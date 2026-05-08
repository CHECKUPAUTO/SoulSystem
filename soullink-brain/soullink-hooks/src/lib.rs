//! Lifecycle hooks for intercepting and transforming SoulLink operations.
//!
//! Ported from IronClaw's hook system. 6 interception points:
//! - **BeforeInbound** — Before processing inbound message
//! - **BeforeToolCall** — Before executing a tool
//! - **BeforeOutbound** — Before sending outbound response
//! - **OnSessionStart** — New session starts
//! - **OnSessionEnd** — Session ends
//! - **TransformResponse** — Transform final response

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Points where hooks can intercept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPoint {
    BeforeInbound,
    BeforeToolCall,
    BeforeOutbound,
    OnSessionStart,
    OnSessionEnd,
    TransformResponse,
}

/// Outcome of a hook evaluation.
#[derive(Debug, Clone)]
pub enum HookOutcome {
    /// Allow the operation to proceed (optionally with modified content).
    Pass(Option<String>),
    /// Reject the operation with a reason.
    Reject(String),
    /// Skip remaining hooks.
    Skip(Option<String>),
}

/// Context provided to hooks.
pub struct HookContext {
    pub point: HookPoint,
    pub content: String,
    pub tool_name: Option<String>,
    pub session_id: String,
    pub metadata: std::collections::HashMap<String, String>,
}

/// A single hook implementation.
#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> u32 { 100 }
    fn points(&self) -> Vec<HookPoint>;
    async fn evaluate(&self, ctx: &HookContext) -> HookOutcome;
}

/// Registry of hooks, executed in priority order.
pub struct HookRegistry {
    hooks: Arc<RwLock<Vec<Arc<dyn Hook>>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn register(&self, hook: Arc<dyn Hook>) {
        let mut hooks = self.hooks.write().await;
        hooks.push(hook);
        hooks.sort_by_key(|h| h.priority());
    }

    pub async fn run(&self, point: HookPoint, mut ctx: HookContext) -> HookOutcome {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            if !hook.points().contains(&point) { continue; }
            match hook.evaluate(&ctx).await {
                HookOutcome::Pass(modified) => {
                    if let Some(m) = modified { ctx.content = m; }
                }
                HookOutcome::Skip(modified) => {
                    if let Some(m) = modified { ctx.content = m; }
                    return HookOutcome::Pass(Some(ctx.content));
                }
                HookOutcome::Reject(reason) => return HookOutcome::Reject(reason),
            }
        }
        HookOutcome::Pass(Some(ctx.content))
    }
}

impl Default for HookRegistry {
    fn default() -> Self { Self::new() }
}

/// Simple logging hook for debugging.
pub struct LoggingHook;

#[async_trait]
impl Hook for LoggingHook {
    fn name(&self) -> &str { "logging" }
    fn priority(&self) -> u32 { 0 }
    fn points(&self) -> Vec<HookPoint> { vec![HookPoint::BeforeInbound, HookPoint::BeforeOutbound] }
    async fn evaluate(&self, ctx: &HookContext) -> HookOutcome {
        tracing::info!(hook = self.name(), point = ?ctx.point, content_len = ctx.content.len(), "hook triggered");
        HookOutcome::Pass(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UppercaseHook;
    #[async_trait]
    impl Hook for UppercaseHook {
        fn name(&self) -> &str { "uppercase" }
        fn priority(&self) -> u32 { 50 }
        fn points(&self) -> Vec<HookPoint> { vec![HookPoint::TransformResponse] }
        async fn evaluate(&self, ctx: &HookContext) -> HookOutcome {
            HookOutcome::Pass(Some(ctx.content.to_uppercase()))
        }
    }

    struct RejectHook;
    #[async_trait]
    impl Hook for RejectHook {
        fn name(&self) -> &str { "reject" }
        fn priority(&self) -> u32 { 10 }
        fn points(&self) -> Vec<HookPoint> { vec![HookPoint::BeforeToolCall] }
        async fn evaluate(&self, _ctx: &HookContext) -> HookOutcome {
            HookOutcome::Reject("blocked".into())
        }
    }

    #[tokio::test]
    async fn transform_hook() {
        let reg = HookRegistry::new();
        reg.register(Arc::new(UppercaseHook)).await;
        let ctx = HookContext { point: HookPoint::TransformResponse, content: "hello".into(), tool_name: None, session_id: "s1".into(), metadata: Default::default() };
        let result = reg.run(HookPoint::TransformResponse, ctx).await;
        match result {
            HookOutcome::Pass(Some(c)) => assert_eq!(c, "HELLO"),
            _ => panic!("expected pass with transformed content"),
        }
    }

    #[tokio::test]
    async fn reject_hook() {
        let reg = HookRegistry::new();
        reg.register(Arc::new(RejectHook)).await;
        let ctx = HookContext { point: HookPoint::BeforeToolCall, content: "rm -rf /".into(), tool_name: Some("shell".into()), session_id: "s1".into(), metadata: Default::default() };
        let result = reg.run(HookPoint::BeforeToolCall, ctx).await;
        assert!(matches!(result, HookOutcome::Reject(_)));
    }

    #[tokio::test]
    async fn no_hooks_for_point() {
        let reg = HookRegistry::new();
        reg.register(Arc::new(UppercaseHook)).await;
        let ctx = HookContext { point: HookPoint::BeforeInbound, content: "test".into(), tool_name: None, session_id: "s1".into(), metadata: Default::default() };
        let result = reg.run(HookPoint::BeforeInbound, ctx).await;
        match result { HookOutcome::Pass(Some(c)) => assert_eq!(c, "test"), _ => panic!("expected pass"), }
    }
}
