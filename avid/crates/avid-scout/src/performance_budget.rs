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

use regex::Regex;

/// Performance budget metrics.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PerformanceBudget {
    pub total_requests: usize,
    pub js_requests: usize,
    pub css_requests: usize,
    pub image_requests: usize,
    pub html_size_bytes: usize,
    pub estimated_page_weight_kb: usize,
    pub score: u8,
}

fn tag_regex(tag: &str) -> Regex {
    Regex::new(&format!(r"<{}[^\u003e]*\u003e", regex::escape(tag))).unwrap()
}

/// Compute performance budget from HTML.
#[must_use]
pub fn compute_budget(html: &str) -> PerformanceBudget {
    let js_requests = tag_regex("script").find_iter(html).count();
    let css_requests = tag_regex("link")
        .find_iter(html)
        .filter(|m| m.as_str().to_lowercase().contains("stylesheet"))
        .count();
    let image_requests = tag_regex("img").find_iter(html).count();
    let total_requests = js_requests + css_requests + image_requests;
    let html_size = html.len();
    let estimated_kb =
        html_size / 1024 + js_requests * 50 + css_requests * 20 + image_requests * 100;

    let mut score = 100u8;
    if total_requests > 50 {
        score = score.saturating_sub(20);
    } else if total_requests > 30 {
        score = score.saturating_sub(10);
    }
    if estimated_kb > 3000 {
        score = score.saturating_sub(20);
    } else if estimated_kb > 1500 {
        score = score.saturating_sub(10);
    }
    if js_requests > 15 {
        score = score.saturating_sub(10);
    }

    PerformanceBudget {
        total_requests,
        js_requests,
        css_requests,
        image_requests,
        html_size_bytes: html_size,
        estimated_page_weight_kb: estimated_kb,
        score,
    }
}
