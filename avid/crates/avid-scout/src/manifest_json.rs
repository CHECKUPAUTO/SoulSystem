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

/// Manifest summary.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ManifestSummary {
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub start_url: Option<String>,
    pub display: Option<String>,
    pub theme_color: Option<String>,
    pub background_color: Option<String>,
    pub icon_count: usize,
}

/// Parse a web app manifest JSON.
#[must_use]
pub fn parse_manifest(json: &str) -> ManifestSummary {
    let parsed: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    ManifestSummary {
        name: parsed
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from),
        short_name: parsed
            .get("short_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        start_url: parsed
            .get("start_url")
            .and_then(|v| v.as_str())
            .map(String::from),
        display: parsed
            .get("display")
            .and_then(|v| v.as_str())
            .map(String::from),
        theme_color: parsed
            .get("theme_color")
            .and_then(|v| v.as_str())
            .map(String::from),
        background_color: parsed
            .get("background_color")
            .and_then(|v| v.as_str())
            .map(String::from),
        icon_count: parsed
            .get("icons")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0),
    }
}
