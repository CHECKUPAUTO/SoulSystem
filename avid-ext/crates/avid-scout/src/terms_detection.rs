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

/// Terms and conditions detection.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TermsDetection {
    pub has_terms_page: bool,
    pub has_terms_link: bool,
    pub url: Option<String>,
    pub has_age_restriction: bool,
    pub has_liability_clause: bool,
    pub has_refund_policy: bool,
}

/// Detect terms and conditions signals.
#[must_use]
pub fn detect_terms(html: &str, links: &[impl AsRef<str>]) -> TermsDetection {
    let lower = html.to_lowercase();
    let mut url = None;
    let mut has_link = false;
    for link in links {
        let l = link.as_ref().to_lowercase();
        if l.contains("terms") || l.contains("conditions") || l.contains("cgv") || l.contains("cgu")
        {
            has_link = true;
            url = Some(link.as_ref().to_string());
        }
    }
    TermsDetection {
        has_terms_page: lower.contains("terms of service")
            || lower.contains("terms and conditions")
            || lower.contains("conditions generales"),
        has_terms_link: has_link,
        url,
        has_age_restriction: lower.contains("age")
            && (lower.contains("18") || lower.contains("13") || lower.contains("minimum age")),
        has_liability_clause: lower.contains("liability")
            || lower.contains("responsabilite")
            || lower.contains("disclaimer"),
        has_refund_policy: lower.contains("refund")
            || lower.contains("remboursement")
            || lower.contains("return policy"),
    }
}
