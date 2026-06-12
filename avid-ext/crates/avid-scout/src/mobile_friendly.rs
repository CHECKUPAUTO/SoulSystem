#![allow(
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::bool_to_int_with_if,
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::use_self,
    clippy::redundant_clone,
    clippy::wildcard_imports,
    clippy::option_if_let_else,
    clippy::manual_split_once,
    clippy::match_wildcard_for_single_variants,
    clippy::single_char_pattern,
    clippy::range_plus_one,
    clippy::unnecessary_map_or,
    clippy::manual_pattern_char_comparison,
    clippy::suboptimal_flops,
    clippy::needless_collect,
    clippy::inefficient_to_string
)]

use regex::Regex;
use std::sync::OnceLock;

/// Mobile-friendliness report.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MobileFriendly {
    pub has_viewport: bool,
    pub viewport_content: Option<String>,
    pub has_media_queries: bool,
    pub has_apple_touch_icon: bool,
    pub has_manifest: bool,
    pub tap_target_size_ok: bool,
}

/// Audit mobile-friendliness signals from HTML.
#[must_use]
pub fn audit_mobile_friendly(html: &str) -> MobileFriendly {
    let lower = html.to_lowercase();
    let viewport = extract_viewport(html);
    MobileFriendly {
        has_viewport: viewport.is_some(),
        viewport_content: viewport.clone(),
        has_media_queries: lower.contains("@media"),
        has_apple_touch_icon: lower.contains("apple-touch-icon"),
        has_manifest: lower.contains("manifest"),
        tap_target_size_ok: viewport.as_ref().map_or(false, |v| {
            v.contains("width=device-width") || v.contains("initial-scale")
        }),
    }
}

fn viewport_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<meta[^\u003e]*name=["']?viewport["']?[^\u003e]*content=["']?([^"'\u003e]+)["']?[^\u003e]*>"#).unwrap()
    })
}

fn extract_viewport(html: &str) -> Option<String> {
    // Optimization: Use pre-compiled regex to avoid redundant compilation
    // and expensive DFA construction for every call to extract_viewport.
    viewport_regex()
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}
