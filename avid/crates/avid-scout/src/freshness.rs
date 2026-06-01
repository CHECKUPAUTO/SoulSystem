#![allow(
    clippy::cast_precision_loss,
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

use chrono::Datelike;
use regex::Regex;
use std::sync::OnceLock;

/// Content freshness signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FreshnessSignals {
    pub dates_found: Vec<String>,
    pub last_modified: Option<String>,
    pub copyright_year: Option<String>,
    pub is_recent: bool,
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}[/-]\d{1,2}[/-]\d{2,4}|\d{4}[/-]\d{1,2}[/-]\d{1,2}|(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+\d{1,2},?\s+\d{4})\b").unwrap())
}

fn copyright_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"©\s*(\d{4})").unwrap())
}

/// Detect dates and freshness signals in text.
#[must_use]
pub fn detect_freshness(text: &str) -> FreshnessSignals {
    let dates: Vec<String> = date_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();

    let copyright = copyright_regex()
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let current_year = chrono::Utc::now().year();
    let is_recent = dates.iter().any(|d| {
        d.contains(&current_year.to_string()) || d.contains(&(current_year - 1).to_string())
    }) || copyright
        .as_ref()
        .map(|y| y.parse::<i32>().unwrap_or(0) >= current_year - 1)
        .unwrap_or(false);

    FreshnessSignals {
        dates_found: dates.clone(),
        last_modified: dates.first().cloned(),
        copyright_year: copyright,
        is_recent,
    }
}
