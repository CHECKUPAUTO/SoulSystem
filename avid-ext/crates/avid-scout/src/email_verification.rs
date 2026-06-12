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

/// Email verification result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EmailVerification {
    pub email: String,
    pub syntax_valid: bool,
    pub has_mx_record: Option<bool>,
    pub is_disposable: bool,
    pub domain: String,
}

fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap())
}

const DISPOSABLE: &[&str] = &[
    "mailinator.com",
    "guerrillamail.com",
    "tempmail.com",
    "10minutemail.com",
    "yopmail.com",
    "throwawaymail.com",
    "getairmail.com",
    "burnermail.io",
];

/// Verify emails found in text.
#[must_use]
pub fn verify_emails(text: &str) -> Vec<EmailVerification> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cap in email_regex().captures_iter(text) {
        let email = cap
            .get(0)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if seen.insert(email.clone()) {
            let parts: Vec<&str> = email.split('@').collect();
            let domain = parts.get(1).unwrap_or(&"").to_string();
            let is_disposable = DISPOSABLE.iter().any(|d| domain.eq_ignore_ascii_case(d));
            results.push(EmailVerification {
                syntax_valid: true,
                has_mx_record: None,
                is_disposable,
                domain: domain.clone(),
                email,
            });
        }
    }
    results
}
