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

/// Analytics and tracking tags detected on a page.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AnalyticsTags {
    pub google_analytics: bool,
    pub google_tag_manager: bool,
    pub facebook_pixel: bool,
    pub hotjar: bool,
    pub mixpanel: bool,
    pub segment: bool,
    pub plausible: bool,
    pub matomo: bool,
    pub hubspot: bool,
    pub intercom: bool,
}

const SIGNATURES: &[(&str, fn(&mut AnalyticsTags))] = &[
    ("googleanalytics", |t| t.google_analytics = true),
    ("google-analytics", |t| t.google_analytics = true),
    ("gtag", |t| t.google_analytics = true),
    ("gtm-", |t| t.google_tag_manager = true),
    ("googletagmanager", |t| t.google_tag_manager = true),
    ("fbevents.js", |t| t.facebook_pixel = true),
    ("facebook.com/tr", |t| t.facebook_pixel = true),
    ("hotjar.com", |t| t.hotjar = true),
    ("mixpanel", |t| t.mixpanel = true),
    ("segment.com", |t| t.segment = true),
    ("segment.io", |t| t.segment = true),
    ("plausible.io", |t| t.plausible = true),
    ("matomo", |t| t.matomo = true),
    ("piwik", |t| t.matomo = true),
    ("hubspot", |t| t.hubspot = true),
    ("intercom.io", |t| t.intercom = true),
    ("intercomcdn", |t| t.intercom = true),
];

/// Detect analytics and tracking scripts in page HTML.
#[must_use]
pub fn detect_analytics(html: &str) -> AnalyticsTags {
    let mut tags = AnalyticsTags::default();
    let lower = html.to_lowercase();
    for (sig, setter) in SIGNATURES {
        if lower.contains(sig) {
            setter(&mut tags);
        }
    }
    tags
}
