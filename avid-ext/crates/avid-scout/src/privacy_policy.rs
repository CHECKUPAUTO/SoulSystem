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

/// Privacy policy detection.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PrivacyPolicy {
    pub has_privacy_page: bool,
    pub has_privacy_link: bool,
    pub url: Option<String>,
    pub mentions_gdpr: bool,
    pub mentions_ccpa: bool,
    pub mentions_cookies: bool,
    pub has_opt_out: bool,
}

/// Detect privacy policy signals from HTML and links.
#[must_use]
pub fn detect_privacy(html: &str, links: &[impl AsRef<str>]) -> PrivacyPolicy {
    let lower = html.to_lowercase();
    let mut url = None;
    let mut has_link = false;
    for link in links {
        let l = link.as_ref().to_lowercase();
        if l.contains("privacy") || l.contains("confidentialite") || l.contains("datenschutz") {
            has_link = true;
            url = Some(link.as_ref().to_string());
        }
    }
    PrivacyPolicy {
        has_privacy_page: lower.contains("privacy policy")
            || lower.contains("politique de confidentialite"),
        has_privacy_link: has_link,
        url,
        mentions_gdpr: lower.contains("gdpr") || lower.contains("general data protection"),
        mentions_ccpa: lower.contains("ccpa") || lower.contains("california consumer"),
        mentions_cookies: lower.contains("cookie policy") || lower.contains("cookie"),
        has_opt_out: lower.contains("opt-out")
            || lower.contains("opt out")
            || lower.contains("unsubscribe"),
    }
}
