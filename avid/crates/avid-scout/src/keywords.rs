#![allow(
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
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

use std::collections::HashMap;

/// Keyword density analysis result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeywordDensity {
    pub top_keywords: Vec<(String, f64)>,
    pub total_words: usize,
}

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "to", "of", "and", "in",
    "that", "have", "it", "for", "not", "on", "with", "he", "as", "you", "do", "at", "this", "but",
    "his", "by", "from", "they", "we", "say", "her", "she", "or", "will", "my", "one", "all",
    "would", "there", "their", "what", "so", "up", "out", "if", "about", "who", "get", "which",
    "go", "me", "when", "make", "can", "like", "time", "no", "just", "him", "know", "take",
    "people", "into", "year", "your", "good", "some", "could", "them", "see", "other", "than",
    "then", "now", "look", "only", "come", "its", "over", "think", "also", "back", "after", "use",
    "two", "how", "our", "work", "first", "well", "way", "even", "new", "want", "because", "any",
    "these", "give", "day", "most", "us",
];

/// Compute keyword density (top 20 keywords with their percentage).
#[must_use]
pub fn compute_keyword_density(text: &str) -> KeywordDensity {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(w))
        .map(String::from)
        .collect();

    let total = words.len();
    let mut counts = HashMap::<String, usize>::new();
    for w in &words {
        *counts.entry(w.clone()).or_insert(0) += 1;
    }

    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.1));

    let top: Vec<(String, f64)> = sorted
        .into_iter()
        .take(20)
        .map(|(w, c)| (w, (c as f64 / total as f64) * 100.0))
        .collect();

    KeywordDensity {
        top_keywords: top,
        total_words: total,
    }
}
