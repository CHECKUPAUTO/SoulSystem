#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
/// Compute the Levenshtein distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0; b_chars.len() + 1]; a_chars.len() + 1];

    for (i, row) in dp.iter_mut().enumerate().take(a_chars.len() + 1) {
        row[0] = i;
    }
    for (j, col) in dp[0].iter_mut().enumerate().take(b_chars.len() + 1) {
        *col = j;
    }

    for i in 1..=a_chars.len() {
        for j in 1..=b_chars.len() {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[a_chars.len()][b_chars.len()]
}

/// A simple diff report between two page versions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageDiff {
    pub distance: usize,
    pub similarity: f64,
}

impl PageDiff {
    pub fn compare(old: &str, new: &str) -> Self {
        let distance = levenshtein(old, new);
        let max_len = old.len().max(new.len()).max(1);
        let similarity = 1.0 - (distance as f64 / max_len as f64);
        Self {
            distance,
            similarity: similarity.clamp(0.0, 1.0),
        }
    }
}
