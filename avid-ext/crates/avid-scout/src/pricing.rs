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

/// Extracted price information.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriceInfo {
    pub currency: String,
    pub amount: Option<f64>,
    pub raw: String,
}

fn price_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"([€£\$]|USD|EUR|GBP)\s*(\d[\d\s\.,]*)|(\d[\d\s\.,]*)\s*([€£\$]|USD|EUR|GBP)")
            .unwrap()
    })
}

/// Extract price mentions from text.
#[must_use]
pub fn extract_prices(text: &str) -> Vec<PriceInfo> {
    let mut results = Vec::new();
    for cap in price_regex().captures_iter(text) {
        let currency = cap
            .get(1)
            .or_else(|| cap.get(4))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let amount_str = cap
            .get(2)
            .or_else(|| cap.get(3))
            .map(|m| m.as_str())
            .unwrap_or("");
        let cleaned: String = amount_str
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
            .collect();
        let amount = cleaned.replace(',', ".").parse::<f64>().ok();
        results.push(PriceInfo {
            currency: match currency.as_str() {
                "$" | "USD" => "USD".to_string(),
                "€" | "EUR" => "EUR".to_string(),
                "£" | "GBP" => "GBP".to_string(),
                _ => currency,
            },
            amount,
            raw: cap
                .get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
        });
    }
    results
}
