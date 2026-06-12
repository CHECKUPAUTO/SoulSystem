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

/// OpenGraph video info.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenGraphVideo {
    pub url: Option<String>,
    pub secure_url: Option<String>,
    pub type_: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn og_video_regex(prop: &str) -> Regex {
    let pattern = if prop.is_empty() {
        r#"<meta[^\u003e]*property=["']?og:video["'\s\u003e][^\u003e]*content=["']?([^"'\u003e]+)["']?[^\u003e]*>"#.to_string()
    } else {
        format!(
            r#"<meta[^\u003e]*property=["']?og:video:{}["']?[^\u003e]*content=["']?([^"'\u003e]+)["']?[^\u003e]*>"#,
            regex::escape(prop)
        )
    };
    Regex::new(&pattern).unwrap()
}

fn find_og(html: &str, prop: &str) -> Option<String> {
    og_video_regex(prop)
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract OpenGraph video tags.
#[must_use]
pub fn extract_og_video(html: &str) -> OpenGraphVideo {
    OpenGraphVideo {
        url: find_og(html, ""),
        secure_url: find_og(html, "secure_url"),
        type_: find_og(html, "type"),
        width: find_og(html, "width").and_then(|s| s.parse().ok()),
        height: find_og(html, "height").and_then(|s| s.parse().ok()),
    }
}
