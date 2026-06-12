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

/// Schema validation report.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SchemaValidation {
    pub total_schemas: usize,
    pub valid_json_ld: usize,
    pub has_required_type: bool,
    pub missing_required_fields: Vec<String>,
    pub score: u8,
}

/// Validate structured data heuristics.
#[must_use]
pub fn validate_schema(structured: &[serde_json::Value]) -> SchemaValidation {
    let mut report = SchemaValidation::default();
    report.total_schemas = structured.len();
    let mut valid = 0;
    let mut missing = Vec::new();
    for item in structured {
        if let Some(t) = item.get("@type").and_then(|v| v.as_str()) {
            valid += 1;
            if t.eq_ignore_ascii_case("Product") {
                if item.get("name").is_none() {
                    missing.push("Product.name".to_string());
                }
                if item.get("offers").is_none() && item.get("aggregateRating").is_none() {
                    missing.push("Product.offers|aggregateRating".to_string());
                }
            }
            if t.eq_ignore_ascii_case("Article") {
                if item.get("headline").is_none() && item.get("name").is_none() {
                    missing.push("Article.headline".to_string());
                }
            }
        }
    }
    report.valid_json_ld = valid;
    report.has_required_type = valid > 0;
    report.missing_required_fields = missing
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    report.score = if report.total_schemas == 0 {
        0
    } else {
        ((valid as f32 / report.total_schemas as f32) * 100.0) as u8
    };
    report
}
