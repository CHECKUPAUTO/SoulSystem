#![allow(
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
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

/// Readability metrics for a text.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReadabilityScore {
    pub flesch_kincaid: f64,
    pub word_count: usize,
    pub sentence_count: usize,
    pub avg_sentence_length: f64,
}

/// Compute Flesch-Kincaid readability score (simplified).
/// Score ranges: 0-30 = very difficult, 90-100 = very easy.
#[must_use]
pub fn compute_readability(text: &str) -> ReadabilityScore {
    let words: Vec<&str> = text.split_whitespace().collect();
    let syllable_count = words.iter().map(|w| count_syllables(w)).sum::<usize>();

    let sentence_count = text
        .split(['.', '!', '?'])
        .filter(|s| !s.trim().is_empty())
        .count()
        .max(1);
    let word_count = words.len().max(1);

    let avg_sentence_length = word_count as f64 / sentence_count as f64;
    let avg_syllables_per_word = syllable_count as f64 / word_count as f64;

    // Flesch Reading Ease = 206.835 - 1.015 * ASL - 84.6 * ASW
    let flesch_kincaid =
        206.835 - 1.015f64.mul_add(avg_sentence_length, 84.6 * avg_syllables_per_word);

    ReadabilityScore {
        flesch_kincaid: flesch_kincaid.clamp(0.0, 100.0),
        word_count,
        sentence_count,
        avg_sentence_length,
    }
}

fn count_syllables(word: &str) -> usize {
    let vowels = "aeiouyAEIOUY";
    let mut count: usize = 0;
    let mut prev_was_vowel = false;
    for ch in word.chars() {
        let is_vowel = vowels.contains(ch);
        if is_vowel && !prev_was_vowel {
            count += 1;
        }
        prev_was_vowel = is_vowel;
    }
    if word.ends_with('e') || word.ends_with('E') {
        count = count.saturating_sub(1usize);
    }
    count.max(1)
}
