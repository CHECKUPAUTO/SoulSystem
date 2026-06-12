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

/// Hreflang audit result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HreflangAudit {
    pub has_self_referencing: bool,
    pub has_x_default: bool,
    pub missing_return_links: Vec<String>,
    pub invalid_lang_codes: Vec<String>,
    pub score: u8,
}

/// Audit hreflang implementation.
#[must_use]
pub fn audit_hreflang(
    hreflangs: &[super::i18n::HreflangEntry],
    canonical_url: &str,
) -> HreflangAudit {
    let mut audit = HreflangAudit::default();
    let valid_langs = vec![
        "en", "fr", "de", "es", "it", "pt", "nl", "pl", "ru", "ja", "zh", "ko", "ar", "tr", "sv",
        "da", "no", "fi", "cs", "hu", "ro", "el", "he", "hi", "th", "vi", "id", "ms", "tl", "uk",
        "hr", "sr", "sk", "sl", "bg", "lt", "lv", "et", "is", "ga", "mt", "eu", "ca", "gl", "sq",
        "mk", "be", "ka", "hy", "az", "uz", "kk", "mn", "ne", "bn", "ta", "te", "ml", "kn", "mr",
        "gu", "pa", "or", "as", "si", "km", "lo", "my", "jw", "su", "tl", "ceb", "hau", "yo", "ig",
        "sw", "am", "so", "rw", "mg", "ny", "sn", "st", "ss", "ts", "tn", "ve", "xh", "zu", "af",
        "zu",
    ];
    let mut has_self = false;
    let mut has_x_default = false;
    for hl in hreflangs {
        if hl.href == canonical_url {
            has_self = true;
        }
        if hl.lang == "x-default" {
            has_x_default = true;
        }
        if !valid_langs.iter().any(|v| *v == hl.lang.as_str()) && hl.lang != "x-default" {
            if !audit.invalid_lang_codes.contains(&hl.lang) {
                audit.invalid_lang_codes.push(hl.lang.clone());
            }
        }
    }
    audit.has_self_referencing = has_self;
    audit.has_x_default = has_x_default;
    let mut score = 100u8;
    if !has_self {
        score = score.saturating_sub(30);
    }
    if !has_x_default {
        score = score.saturating_sub(20);
    }
    if !audit.invalid_lang_codes.is_empty() {
        score = score.saturating_sub(20);
    }
    audit.score = score;
    audit
}
