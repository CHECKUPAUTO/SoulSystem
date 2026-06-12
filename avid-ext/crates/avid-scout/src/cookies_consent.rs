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

/// Detected cookie consent banner information.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CookieConsent {
    pub has_banner: bool,
    pub cmp_detected: Option<String>,
    pub has_reject_button: bool,
    pub has_accept_all_button: bool,
}

const CMP_SIGNATURES: &[(&str, &str)] = &[
    ("cookiebot", "Cookiebot"),
    ("onetrust", "OneTrust"),
    ("quantcast", "Quantcast"),
    ("didomi", "Didomi"),
    ("axeptio", "Axeptio"),
    ("tarteaucitron", "Tarte au Citron"),
    ("osano", "Osano"),
    ("iubenda", "iubenda"),
    ("cookieyes", "CookieYes"),
];

/// Detect cookie consent banners and CMPs in page HTML/text.
#[must_use]
pub fn detect_cookie_consent(html: &str, _text: &str) -> CookieConsent {
    let lower = html.to_lowercase();
    let mut cmp = None;
    for (sig, name) in CMP_SIGNATURES {
        if lower.contains(sig) {
            cmp = Some(name.to_string());
            break;
        }
    }
    let has_banner = lower.contains("cookie")
        && (lower.contains("consent") || lower.contains("accept") || lower.contains("gdpr"));
    let has_reject =
        lower.contains("reject") || lower.contains("refuse") || lower.contains("decline");
    let has_accept = lower.contains("accept all")
        || lower.contains("accept all cookies")
        || lower.contains("allow all");

    CookieConsent {
        has_banner: has_banner || cmp.is_some(),
        cmp_detected: cmp,
        has_reject_button: has_reject,
        has_accept_all_button: has_accept,
    }
}
