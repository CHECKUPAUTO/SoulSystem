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

/// Simulated PageSpeed Insights score.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PageSpeedInsights {
    pub performance: u8,
    pub accessibility: u8,
    pub best_practices: u8,
    pub seo: u8,
    pub overall: u8,
}

/// Simulate PSI scores from page signals.
#[must_use]
pub fn simulate_psi(
    html_size_kb: usize,
    image_count: usize,
    script_count: usize,
    has_viewport: bool,
) -> PageSpeedInsights {
    let perf = (100usize)
        .saturating_sub(html_size_kb / 10 + image_count * 2 + script_count * 3)
        .max(10)
        .min(100) as u8;
    let a11y = if has_viewport { 85u8 } else { 60u8 };
    let bp = 90u8;
    let seo = if has_viewport { 92u8 } else { 70u8 };
    let overall = ((perf as u16 + a11y as u16 + bp as u16 + seo as u16) / 4).min(100) as u8;
    PageSpeedInsights {
        performance: perf,
        accessibility: a11y,
        best_practices: bp,
        seo,
        overall,
    }
}
