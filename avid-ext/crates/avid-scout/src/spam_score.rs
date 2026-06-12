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

/// Spam score result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SpamScore {
    pub score: u8,
    pub flags: Vec<String>,
}

/// Compute a heuristic spam score (0-100) for page text.
#[must_use]
pub fn compute_spam_score(text: &str, link_count: usize, image_count: usize) -> SpamScore {
    let mut score = 0u8;
    let mut flags = Vec::new();
    let lower = text.to_lowercase();

    let words: Vec<&str> = lower.split_whitespace().collect();
    let total_words = words.len().max(1);

    // Keyword stuffing
    let mut word_freq = std::collections::HashMap::new();
    for w in &words {
        *word_freq.entry(*w).or_insert(0) += 1;
    }
    let max_freq = word_freq.values().copied().max().unwrap_or(0);
    if max_freq > total_words / 10 {
        score = score.saturating_add(20);
        flags.push("keyword_stuffing".to_string());
    }

    // Too many links relative to words
    if link_count > total_words / 5 {
        score = score.saturating_add(15);
        flags.push("link_heavy".to_string());
    }

    // Hidden text signals
    if lower.contains("display:none") || lower.contains("visibility:hidden") {
        score = score.saturating_add(15);
        flags.push("hidden_text".to_string());
    }

    // Excessive caps
    let caps_ratio =
        text.chars().filter(|c| c.is_ascii_uppercase()).count() as f64 / text.len().max(1) as f64;
    if caps_ratio > 0.3 {
        score = score.saturating_add(10);
        flags.push("excessive_caps".to_string());
    }

    // Too many images
    if image_count > total_words / 3 {
        score = score.saturating_add(10);
        flags.push("image_heavy".to_string());
    }

    // Suspicious phrases
    let suspicious = [
        "click here",
        "buy now",
        "act now",
        "limited time",
        "free money",
        "make money fast",
        "no credit check",
        "100% guaranteed",
    ];
    for phrase in &suspicious {
        if lower.contains(phrase) {
            score = score.saturating_add(5);
            if !flags.iter().any(|f| f == "suspicious_phrases") {
                flags.push("suspicious_phrases".to_string());
            }
        }
    }

    SpamScore { score, flags }
}
