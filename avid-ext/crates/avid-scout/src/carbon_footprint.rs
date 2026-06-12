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

/// Carbon footprint estimate (grams of CO2 per page view).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CarbonFootprint {
    pub estimated_co2_g: f64,
    pub page_weight_kb: usize,
    pub is_green_hosted: bool,
    pub grade: char,
}

/// Estimate carbon footprint from page weight.
#[must_use]
pub fn estimate_carbon(page_weight_kb: usize) -> CarbonFootprint {
    // Rough estimate: ~0.5g CO2 per MB (data transfer + datacenter)
    let co2 = (page_weight_kb as f64 / 1024.0) * 0.5;
    let grade = if co2 < 0.2 {
        'A'
    } else if co2 < 0.5 {
        'B'
    } else if co2 < 1.0 {
        'C'
    } else {
        'D'
    };
    CarbonFootprint {
        estimated_co2_g: co2,
        page_weight_kb,
        is_green_hosted: false,
        grade,
    }
}
