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

/// Internal search signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct InternalSearch {
    pub has_search_form: bool,
    pub search_url: Option<String>,
    pub search_param: Option<String>,
    pub has_autocomplete: bool,
}

/// Detect internal site search.
#[must_use]
pub fn detect_internal_search(html: &str, forms: &[super::forms::FormInfo]) -> InternalSearch {
    let lower = html.to_lowercase();
    let mut result = InternalSearch::default();
    for form in forms {
        if let Some(action) = &form.action {
            if action.to_lowercase().contains("search") || action.to_lowercase().contains("q=") {
                result.has_search_form = true;
                result.search_url = Some(action.clone());
            }
        }
        for field in &form.fields {
            if field.name.to_lowercase().contains("q")
                || field.name.to_lowercase().contains("search")
                || field.name.to_lowercase().contains("query")
            {
                result.has_search_form = true;
                result.search_param = Some(field.name.clone());
            }
        }
    }
    if lower.contains("autocomplete") || lower.contains("typeahead") || lower.contains("suggest") {
        result.has_autocomplete = true;
    }
    result
}
