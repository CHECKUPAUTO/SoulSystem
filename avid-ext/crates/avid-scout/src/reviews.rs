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

/// Extracted review metadata.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Reviews {
    pub count: Option<usize>,
    pub average_rating: Option<f32>,
    pub review_snippets: Vec<String>,
}

fn rating_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(\d(?:\.\d)?)\s*\/\s*5|\b(\d(?:\.\d)?)\s*out\s*of\s*5\b").unwrap()
    })
}

fn review_count_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+)\s*(reviews?|avis|évaluations?)").unwrap())
}

/// Extract review signals from text.
#[must_use]
pub fn extract_reviews(text: &str) -> Reviews {
    let mut avg = None;
    let mut count = None;
    let mut snippets = Vec::new();

    for cap in rating_regex().captures_iter(text) {
        let rating = cap
            .get(1)
            .or_else(|| cap.get(2))
            .and_then(|m| m.as_str().parse::<f32>().ok());
        if let Some(r) = rating {
            avg = Some(avg.map(|a: f32| (a + r) / 2.0).unwrap_or(r));
        }
    }

    for cap in review_count_regex().captures_iter(text) {
        if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) {
            count = Some(count.unwrap_or(0) + n);
        }
    }

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() > 30 && trimmed.len() < 500 && (trimmed.starts_with("\"")) {
            snippets.push(trimmed.to_string());
        }
    }

    Reviews {
        count,
        average_rating: avg,
        review_snippets: snippets.into_iter().take(10).collect(),
    }
}
