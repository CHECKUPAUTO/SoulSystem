#![allow(
    clippy::needless_collect,
    clippy::cast_precision_loss,
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

/// Sentiment analysis result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sentiment {
    pub score: f64,
    pub label: SentimentLabel,
    pub positive_words: Vec<String>,
    pub negative_words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SentimentLabel {
    Positive,
    Neutral,
    Negative,
}

const POSITIVE: &[&str] = &[
    "good",
    "great",
    "excellent",
    "amazing",
    "love",
    "best",
    "fantastic",
    "wonderful",
    "perfect",
    "happy",
    "success",
    "win",
    "beautiful",
    "easy",
    "fast",
    "reliable",
    "secure",
    "powerful",
    "innovative",
    "smart",
];

const NEGATIVE: &[&str] = &[
    "bad",
    "terrible",
    "awful",
    "hate",
    "worst",
    "horrible",
    "slow",
    "broken",
    "bug",
    "crash",
    "error",
    "fail",
    "ugly",
    "difficult",
    "unreliable",
    "insecure",
    "weak",
    "outdated",
    "stupid",
    "expensive",
];

/// Analyse sentiment of text using keyword matching.
#[must_use]
pub fn analyse_sentiment(text: &str) -> Sentiment {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    let mut pos_found = Vec::new();
    let mut neg_found = Vec::new();

    for word in &words {
        let clean = word.trim_matches(|c: char| !c.is_alphabetic());
        if POSITIVE.contains(&clean) {
            pos_found.push(clean.to_string());
        }
        if NEGATIVE.contains(&clean) {
            neg_found.push(clean.to_string());
        }
    }

    let pos_count = pos_found.len() as f64;
    let neg_count = neg_found.len() as f64;
    let total = pos_count + neg_count;

    let score = if total > 0.0 {
        (pos_count - neg_count) / total
    } else {
        0.0
    };

    let label = if score > 0.1 {
        SentimentLabel::Positive
    } else if score < -0.1 {
        SentimentLabel::Negative
    } else {
        SentimentLabel::Neutral
    };

    Sentiment {
        score,
        label,
        positive_words: pos_found,
        negative_words: neg_found,
    }
}
