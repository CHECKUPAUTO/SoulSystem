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

/// Content quality signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ContentQuality {
    pub word_count: usize,
    pub sentence_count: usize,
    pub avg_word_length: f32,
    pub paragraph_count: usize,
    pub has_headings: bool,
    pub has_lists: bool,
    pub has_tables: bool,
    pub image_to_text_ratio: f32,
    pub score: u8,
}

/// Assess content quality from text and HTML.
#[must_use]
pub fn assess_quality(text: &str, html: &str, image_count: usize) -> ContentQuality {
    let words: Vec<&str> = text.split_whitespace().collect();
    let word_count = words.len();
    let sentences = text
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .filter(|s| !s.trim().is_empty())
        .count();
    let avg_word_length = if word_count > 0 {
        text.chars().filter(|c| c.is_alphabetic()).count() as f32 / word_count as f32
    } else {
        0.0
    };
    let paragraphs = text.split("\n\n").filter(|p| !p.trim().is_empty()).count();
    let has_headings = html.to_lowercase().contains("<h1")
        || html.to_lowercase().contains("<h2")
        || html.to_lowercase().contains("<h3");
    let has_lists = html.to_lowercase().contains("<ul") || html.to_lowercase().contains("<ol");
    let has_tables = html.to_lowercase().contains("<table");
    let image_to_text_ratio = if word_count > 0 {
        image_count as f32 / word_count as f32
    } else {
        0.0
    };

    let mut score = 50u8;
    if word_count > 300 {
        score = score.saturating_add(15);
    }
    if sentences > 10 {
        score = score.saturating_add(10);
    }
    if has_headings {
        score = score.saturating_add(10);
    }
    if has_lists {
        score = score.saturating_add(5);
    }
    if has_tables {
        score = score.saturating_add(5);
    }
    if image_to_text_ratio > 0.01 && image_to_text_ratio < 0.2 {
        score = score.saturating_add(5);
    }

    ContentQuality {
        word_count,
        sentence_count: sentences,
        avg_word_length,
        paragraph_count: paragraphs,
        has_headings,
        has_lists,
        has_tables,
        image_to_text_ratio,
        score,
    }
}
