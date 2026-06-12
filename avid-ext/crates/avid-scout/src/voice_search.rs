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

/// Voice search optimisation signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct VoiceSearch {
    pub has_faq_schema: bool,
    pub has_how_to_schema: bool,
    pub avg_sentence_length: f32,
    pub conversational_tone: bool,
    pub has_quick_answers: bool,
    pub score: u8,
}

/// Audit voice-search readiness.
#[must_use]
pub fn audit_voice_search(text: &str, structured: &[serde_json::Value]) -> VoiceSearch {
    let sentences: Vec<&str> = text
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .filter(|s| !s.trim().is_empty())
        .collect();
    let avg_sentence_length = if sentences.is_empty() {
        0.0
    } else {
        sentences
            .iter()
            .map(|s| s.split_whitespace().count())
            .sum::<usize>() as f32
            / sentences.len() as f32
    };
    let has_faq = structured.iter().any(|v| {
        v.get("@type")
            .and_then(|t| t.as_str())
            .map_or(false, |t| t.eq_ignore_ascii_case("FAQPage"))
    });
    let has_howto = structured.iter().any(|v| {
        v.get("@type")
            .and_then(|t| t.as_str())
            .map_or(false, |t| t.eq_ignore_ascii_case("HowTo"))
    });
    let conversational =
        text.to_lowercase().contains("you") || text.to_lowercase().contains("your");
    let quick_answers = text.to_lowercase().starts_with("yes")
        || text.to_lowercase().starts_with("no")
        || text.contains(":");

    let mut score = 0u8;
    if has_faq {
        score = score.saturating_add(30);
    }
    if has_howto {
        score = score.saturating_add(20);
    }
    if avg_sentence_length > 0.0 && avg_sentence_length < 20.0 {
        score = score.saturating_add(20);
    }
    if conversational {
        score = score.saturating_add(15);
    }
    if quick_answers {
        score = score.saturating_add(15);
    }

    VoiceSearch {
        has_faq_schema: has_faq,
        has_how_to_schema: has_howto,
        avg_sentence_length,
        conversational_tone: conversational,
        has_quick_answers: quick_answers,
        score,
    }
}
