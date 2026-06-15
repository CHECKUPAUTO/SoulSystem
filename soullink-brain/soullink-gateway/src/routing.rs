//! Cost-aware provider routing.
//!
//! Bridges the gateway's provider registry to the learned router in
//! `avid_model_router`: each registered provider becomes a `ModelProfile`
//! (using the optional cost/capability metadata on [`ProviderConfig`]), and the
//! [`CostAwareRouter`] picks the cheapest provider that clears the query's
//! predicted difficulty bar. When no provider carries routing metadata the
//! router still works (it falls back to the cheapest capable provider), and the
//! caller falls back to round-robin if routing yields nothing.

use std::str::FromStr;

use avid_model_router::{
    Capability, CostAwareRouter, ModelProfile, ModelRegistry, RouterParams, RoutingDecision,
};

use crate::provider::ProviderConfig;

/// Build a `ModelProfile` from a gateway provider config.
fn profile_from(name: &str, cfg: &ProviderConfig) -> ModelProfile {
    let capabilities = parse_caps(&cfg.capabilities);
    ModelProfile {
        name: name.to_string(),
        provider: cfg.provider_type.clone(),
        base_url: cfg.base_url.clone(),
        capabilities,
        max_tokens: cfg.max_tokens,
        cost_per_1k_tokens: cfg.cost_per_1k_tokens,
        priority: 10,
        avg_latency_ms: cfg.avg_latency_ms,
    }
}

/// Parse capability strings, silently dropping unknown ones.
pub fn parse_caps(raw: &[String]) -> Vec<Capability> {
    raw.iter()
        .filter_map(|c| Capability::from_str(c).ok())
        .collect()
}

/// Construct a cost-aware router over `(provider name, config)` entries with
/// explicit (possibly trained) parameters.
pub fn build_router_with(
    providers: &[(String, ProviderConfig)],
    params: RouterParams,
) -> CostAwareRouter {
    let mut registry = ModelRegistry::new();
    for (name, cfg) in providers {
        // Duplicate names can't occur (registry keys are unique); ignore errors.
        let _ = registry.register(profile_from(name, cfg));
    }
    CostAwareRouter::new(registry, params)
}

/// Construct a cost-aware router with default parameters.
pub fn build_router(providers: &[(String, ProviderConfig)]) -> CostAwareRouter {
    build_router_with(providers, RouterParams::default())
}

/// Route a query to a provider name. `None` means no provider satisfied the
/// requested capabilities (caller should fall back to round-robin).
pub fn route(
    providers: &[(String, ProviderConfig)],
    query: &str,
    capabilities: &[Capability],
) -> Option<RoutingDecision> {
    if providers.is_empty() {
        return None;
    }
    build_router(providers).route(query, capabilities)
}

/// Route a query to a primary provider plus an optional escalation provider —
/// the cheapest strictly-stronger provider to retry with when the primary's
/// answer fails or proves unsatisfactory. `None` when no provider is capable
/// (caller should fall back to round-robin). The escalation is `None` when the
/// primary is already the strongest capable provider.
pub fn route_with_escalation(
    providers: &[(String, ProviderConfig)],
    query: &str,
    capabilities: &[Capability],
) -> Option<(String, Option<String>)> {
    route_with_escalation_with(providers, query, capabilities, RouterParams::default())
}

/// As [`route_with_escalation`], but with explicit (possibly trained) router
/// parameters so the gateway can route with a learned difficulty model.
pub fn route_with_escalation_with(
    providers: &[(String, ProviderConfig)],
    query: &str,
    capabilities: &[Capability],
    params: RouterParams,
) -> Option<(String, Option<String>)> {
    if providers.is_empty() {
        return None;
    }
    let router = build_router_with(providers, params);
    let decision = router.route(query, capabilities)?;
    let primary = decision.profile.name.clone();
    let escalation = router
        .escalation_target(&decision, capabilities)
        .map(|p| p.name);
    Some((primary, escalation))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(cost: f64, latency: u64, max_tokens: u64, caps: &[&str]) -> ProviderConfig {
        ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec![],
            default_model: None,
            timeout_secs: 30,
            cost_per_1k_tokens: cost,
            avg_latency_ms: latency,
            max_tokens,
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn cost_averse_easy_query_picks_cheapest() {
        let providers = vec![
            (
                "tiny".into(),
                cfg(0.0, 200, 8192, &["fast-chat", "summarization"]),
            ),
            (
                "frontier".into(),
                cfg(3.0, 4000, 200000, &["analysis", "planning", "fast-chat"]),
            ),
        ];
        // Maximum cost aversion ⇒ always the cheapest *capable* provider.
        let mut router = build_router(&providers);
        let mut params = router.params().clone();
        params.cost_aversion = 1.0;
        router.set_params(params);
        let d = router
            .route("hi, thanks!", &parse_caps(&["chat".into()]))
            .unwrap();
        assert_eq!(d.profile.name, "tiny");
    }

    #[test]
    fn routes_hard_query_to_capable_provider() {
        let providers = vec![
            ("tiny".into(), cfg(0.0, 200, 8192, &["fast-chat"])),
            (
                "frontier".into(),
                cfg(3.0, 4000, 200000, &["analysis", "planning"]),
            ),
        ];
        let caps = parse_caps(&["analysis".into()]);
        let d = route(
            &providers,
            "Analyze and prove the root cause step by step then design a fix",
            &caps,
        )
        .unwrap();
        assert_eq!(d.profile.name, "frontier");
    }

    #[test]
    fn no_capable_provider_returns_none() {
        let providers = vec![("tiny".into(), cfg(0.0, 200, 8192, &["fast-chat"]))];
        let caps = parse_caps(&["analysis".into()]);
        assert!(route(&providers, "deep analysis", &caps).is_none());
    }

    #[test]
    fn empty_fleet_returns_none() {
        assert!(route(&[], "anything", &[]).is_none());
    }

    #[test]
    fn route_with_escalation_offers_stronger_provider() {
        let providers = vec![
            ("tiny".into(), cfg(0.0, 200, 8192, &["fast-chat"])),
            (
                "frontier".into(),
                cfg(3.0, 4000, 200000, &["fast-chat", "analysis", "planning"]),
            ),
        ];
        let (primary, escalation) =
            route_with_escalation(&providers, "hi there", &parse_caps(&["fast-chat".into()]))
                .unwrap();
        // Contract: the escalation is the stronger provider when the cheap one
        // is chosen, and absent when the strongest is already chosen.
        match primary.as_str() {
            "tiny" => assert_eq!(escalation.as_deref(), Some("frontier")),
            "frontier" => assert!(escalation.is_none()),
            other => panic!("unexpected primary {other}"),
        }
    }

    #[test]
    fn route_with_escalation_none_when_strongest_chosen() {
        let providers = vec![(
            "frontier".into(),
            cfg(3.0, 4000, 200000, &["fast-chat", "analysis"]),
        )];
        let (primary, escalation) =
            route_with_escalation(&providers, "hi", &parse_caps(&["fast-chat".into()])).unwrap();
        assert_eq!(primary, "frontier");
        assert!(escalation.is_none());
    }
}
