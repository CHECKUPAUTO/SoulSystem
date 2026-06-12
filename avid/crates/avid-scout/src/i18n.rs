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
use std::sync::OnceLock;

/// Hreflang link entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HreflangEntry {
    pub href: String,
    pub lang: String,
}

/// i18n signals extracted from a page.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct I18nSignals {
    pub hreflangs: Vec<HreflangEntry>,
    pub html_lang: Option<String>,
    pub canonical_lang: Option<String>,
}

fn hreflang_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<link[^\u003e]*rel=["']?alternate["']?[^\u003e]*hreflang=["']?([^"'\s\u003e]+)["']?[^\u003e]*href=["']?([^"'\s\u003e]+)["']?[^\u003e]*>|<link[^\u003e]*hreflang=["']?([^"'\s\u003e]+)["']?[^\u003e]*href=["']?([^"'\s\u003e]+)["']?[^\u003e]*rel=["']?alternate["']?[^\u003e]*>"#).unwrap())
}

/// Extract i18n signals from HTML.
#[must_use]
pub fn extract_i18n(html: &str, html_lang: Option<String>) -> I18nSignals {
    let mut hreflangs = Vec::new();
    for cap in hreflang_regex().captures_iter(html) {
        let lang = cap
            .get(1)
            .or_else(|| cap.get(3))
            .map(|m| m.as_str().to_string());
        let href = cap
            .get(2)
            .or_else(|| cap.get(4))
            .map(|m| m.as_str().to_string());
        if let (Some(l), Some(h)) = (lang, href) {
            hreflangs.push(HreflangEntry { lang: l, href: h });
        }
    }
    let canonical = html_lang.clone();
    I18nSignals {
        hreflangs,
        html_lang,
        canonical_lang: canonical,
    }
}
