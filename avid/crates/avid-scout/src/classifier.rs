#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use crate::metadata::PageMetadata;

/// Page classification categories.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageCategory {
    Article,
    Product,
    Documentation,
    Landing,
    Forum,
    Error,
    Search,
    Unknown,
}

/// Classify a page based on metadata, URL patterns, and heuristics.
pub fn classify_page(url: &str, meta: &PageMetadata, has_forms: bool) -> PageCategory {
    let url_lower = url.to_lowercase();

    if url_lower.contains("/article/")
        || url_lower.contains("/blog/")
        || meta
            .og_title
            .as_deref()
            .map(|s| s.contains("Article"))
            .unwrap_or(false)
        || url_lower.contains("/news/")
    {
        return PageCategory::Article;
    }

    if url_lower.contains("/product/")
        || url_lower.contains("/item/")
        || url_lower.contains("/shop/")
        || url_lower.contains("/buy/")
    {
        return PageCategory::Product;
    }

    if url_lower.contains("/docs/")
        || url_lower.contains("/documentation/")
        || url_lower.contains("/api/")
        || url_lower.contains("/reference/")
    {
        return PageCategory::Documentation;
    }

    if url_lower.contains("/forum/")
        || url_lower.contains("/topic/")
        || url_lower.contains("/thread/")
        || url_lower.contains("/discussion/")
    {
        return PageCategory::Forum;
    }

    if url_lower.contains("/search") || url_lower.contains("?q=") || url_lower.contains("?query=") {
        return PageCategory::Search;
    }

    if url_lower.contains("/404")
        || url_lower.contains("/error")
        || url_lower.contains("/not-found")
    {
        return PageCategory::Error;
    }

    if has_forms
        && meta
            .title
            .as_deref()
            .map(|t| t.to_lowercase().contains("sign up"))
            .unwrap_or(false)
    {
        return PageCategory::Landing;
    }

    PageCategory::Unknown
}
