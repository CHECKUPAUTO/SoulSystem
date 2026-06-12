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

/// Infinite scroll detection.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct InfiniteScroll {
    pub detected: bool,
    pub load_more_button: bool,
    pub scroll_event_listener: bool,
    pub pagination_hidden: bool,
}

/// Detect infinite scroll patterns.
#[must_use]
pub fn detect_infinite_scroll(html: &str) -> InfiniteScroll {
    let lower = html.to_lowercase();
    InfiniteScroll {
        detected: lower.contains("infinite scroll")
            || lower.contains("infinitescroll")
            || lower.contains("load more"),
        load_more_button: lower.contains("load more")
            || lower.contains("show more")
            || lower.contains("afficher plus"),
        scroll_event_listener: lower.contains("scroll") && lower.contains("addeventlistener"),
        pagination_hidden: lower.contains("display:none") && lower.contains("pagination"),
    }
}
