#![allow(
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

use regex::Regex;
use std::sync::OnceLock;

/// Extracted contact information from a page.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Contacts {
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub social_links: Vec<String>,
}

fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap())
}

fn phone_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\+?[\d\s\-\(\)]{7,20})").unwrap())
}

/// Extract emails, phone numbers, and social media links from text.
#[must_use]
pub fn extract_contacts(text: &str, links: &[String]) -> Contacts {
    let mut emails: Vec<String> = email_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect();
    emails.sort_unstable();
    emails.dedup();

    let mut phones: Vec<String> = phone_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    phones.sort_unstable();
    phones.dedup();

    let social_domains = [
        "twitter.com",
        "x.com",
        "facebook.com",
        "instagram.com",
        "linkedin.com",
        "github.com",
        "youtube.com",
        "tiktok.com",
        "pinterest.com",
    ];
    let social_links: Vec<String> = links
        .iter()
        .filter(|l| social_domains.iter().any(|d| l.to_lowercase().contains(d)))
        .cloned()
        .collect();

    Contacts {
        emails,
        phones,
        social_links,
    }
}
