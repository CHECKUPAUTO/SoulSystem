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

/// Twitter Card / social meta information.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SocialMeta {
    pub twitter_card: Option<String>,
    pub twitter_site: Option<String>,
    pub twitter_title: Option<String>,
    pub twitter_description: Option<String>,
    pub twitter_image: Option<String>,
    pub fb_app_id: Option<String>,
    pub og_locale: Option<String>,
    pub og_type: Option<String>,
}

fn meta_regex(name: &str) -> Regex {
    Regex::new(&format!(
        r#"<meta[^\u003e]*(?:name|property)=["']?{}["']?[^\u003e]*content=["']?([^"'\u003e]+)["']?[^\u003e]*>"#,
        regex::escape(name)
    )).unwrap()
}

fn find_meta(html: &str, name: &str) -> Option<String> {
    meta_regex(name)
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract Twitter Card and Facebook OpenGraph meta tags.
#[must_use]
pub fn extract_social_meta(html: &str) -> SocialMeta {
    SocialMeta {
        twitter_card: find_meta(html, "twitter:card"),
        twitter_site: find_meta(html, "twitter:site"),
        twitter_title: find_meta(html, "twitter:title"),
        twitter_description: find_meta(html, "twitter:description"),
        twitter_image: find_meta(html, "twitter:image"),
        fb_app_id: find_meta(html, "fb:app_id"),
        og_locale: find_meta(html, "og:locale"),
        og_type: find_meta(html, "og:type"),
    }
}
