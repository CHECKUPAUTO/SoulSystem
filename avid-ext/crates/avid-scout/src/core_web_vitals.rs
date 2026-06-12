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

/// Estimated Core Web Vitals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CoreWebVitals {
    pub estimated_lcp_ms: u32,
    pub estimated_fid_ms: u32,
    pub estimated_cls: f32,
    pub lcp_grade: char,
    pub fid_grade: char,
    pub cls_grade: char,
    pub overall_grade: char,
}

/// Estimate Core Web Vitals from HTML size and resource count.
#[must_use]
pub fn estimate_cwv(
    html_size_bytes: usize,
    script_count: usize,
    image_count: usize,
) -> CoreWebVitals {
    let estimated_lcp = (html_size_bytes / 1024 + image_count * 100)
        .max(500)
        .min(10000) as u32;
    let estimated_fid = (script_count * 50).max(10).min(500) as u32;
    let estimated_cls = if image_count > 10 { 0.25f32 } else { 0.05f32 };

    let lcp_grade = if estimated_lcp <= 2500 {
        'A'
    } else if estimated_lcp <= 4000 {
        'B'
    } else {
        'C'
    };
    let fid_grade = if estimated_fid <= 100 {
        'A'
    } else if estimated_fid <= 300 {
        'B'
    } else {
        'C'
    };
    let cls_grade = if estimated_cls <= 0.1 {
        'A'
    } else if estimated_cls <= 0.25 {
        'B'
    } else {
        'C'
    };

    let overall = match (lcp_grade, fid_grade, cls_grade) {
        ('A', 'A', 'A') => 'A',
        ('C', _, _) | (_, 'C', _) | (_, _, 'C') => 'C',
        _ => 'B',
    };

    CoreWebVitals {
        estimated_lcp_ms: estimated_lcp,
        estimated_fid_ms: estimated_fid,
        estimated_cls,
        lcp_grade,
        fid_grade,
        cls_grade,
        overall_grade: overall,
    }
}
