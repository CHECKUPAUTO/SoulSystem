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

/// Lazy loading signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LazyLoading {
    pub has_lazy_images: bool,
    pub has_intersection_observer: bool,
    pub has_native_lazy: bool,
    pub has_loading_attribute: bool,
    pub estimated_lazy_images: usize,
}

/// Detect lazy loading patterns.
#[must_use]
pub fn detect_lazy_loading(html: &str) -> LazyLoading {
    let lower = html.to_lowercase();
    let native_lazy =
        lower.matches("loading=\"lazy\"").count() + lower.matches("loading='lazy'").count();
    LazyLoading {
        has_lazy_images: lower.contains("lazy") && lower.contains("img"),
        has_intersection_observer: lower.contains("intersectionobserver"),
        has_native_lazy: native_lazy > 0,
        has_loading_attribute: native_lazy > 0 || lower.contains("loading=\"eager\""),
        estimated_lazy_images: native_lazy,
    }
}
