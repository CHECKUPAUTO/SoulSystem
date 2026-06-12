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

use regex::Regex;
use std::sync::OnceLock;

/// Named entities discovered in text.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Entities {
    pub persons: Vec<String>,
    pub organizations: Vec<String>,
    pub locations: Vec<String>,
    pub currencies: Vec<String>,
}

fn person_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Z][a-z]+\s+[A-Z][a-z]+").unwrap())
}

fn org_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:Inc\.?|LLC|Ltd\.?|Corp\.?|GmbH|SA|AG|BV|PLC)\b").unwrap())
}

fn location_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:Paris|London|Berlin|New\s+York|Tokyo|San\s+Francisco|Lyon|Marseille|Nice|Bordeaux|Lille|Nantes|Strasbourg|Montpellier)\b").unwrap())
}

fn currency_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d+[\.,]?\d*\s?(€|\$|USD|EUR|GBP|£|\b[eE][uU][rR][oO]?s?\b|\b[dD][oO][lL][lL][aA][rR][sS]?\b)\b").unwrap())
}

/// Extract named entities from text using regex heuristics.
#[must_use]
pub fn extract_entities(text: &str) -> Entities {
    let persons: Vec<String> = person_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    let organizations: Vec<String> = org_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    let locations: Vec<String> = location_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    let currencies: Vec<String> = currency_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    Entities {
        persons,
        organizations,
        locations,
        currencies,
    }
}
