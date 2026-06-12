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

/// International phone number.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InternationalPhone {
    pub raw: String,
    pub country_hint: Option<String>,
}

fn phone_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\+\d[\d\s\-\(\)]{7,20}").unwrap())
}

/// Extract international phone numbers.
#[must_use]
pub fn extract_international_phones(text: &str) -> Vec<InternationalPhone> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cap in phone_regex().captures_iter(text) {
        let raw = cap
            .get(0)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let cleaned = raw
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect::<String>();
        if seen.insert(cleaned.clone()) && cleaned.len() >= 8 {
            results.push(InternationalPhone {
                raw,
                country_hint: None,
            });
        }
    }
    results
}
