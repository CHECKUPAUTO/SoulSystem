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

/// Search suggestion box signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SearchSuggestions {
    pub has_sitelinks_searchbox: bool,
    pub search_url: Option<String>,
    pub search_target: Option<String>,
    pub search_query_input: Option<String>,
}

/// Detect search suggestion schema.
#[must_use]
pub fn detect_search_suggestions(structured: &[serde_json::Value]) -> SearchSuggestions {
    let mut result = SearchSuggestions::default();
    for item in structured {
        if let Some(t) = item.get("@type").and_then(|v| v.as_str()) {
            if t.eq_ignore_ascii_case("WebSite") {
                if let Some(pot) = item.get("potentialAction") {
                    if let Some(action) = pot.get("@type").and_then(|v| v.as_str()) {
                        if action.eq_ignore_ascii_case("SearchAction") {
                            result.has_sitelinks_searchbox = true;
                            result.search_url =
                                pot.get("target").and_then(|v| v.as_str()).map(String::from);
                            result.search_target = result.search_url.clone();
                            result.search_query_input = pot
                                .get("query-input")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                        }
                    }
                }
            }
        }
    }
    result
}
