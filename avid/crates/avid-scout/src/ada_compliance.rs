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

/// ADA / Section 508 compliance report.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AdaCompliance {
    pub has_skip_links: bool,
    pub has_aria_landmarks: bool,
    pub has_accessible_forms: bool,
    pub has_transcripts: bool,
    pub has_captions: bool,
    pub keyboard_navigable: bool,
    pub score: u8,
}

/// Audit ADA/Section 508 compliance.
#[must_use]
pub fn audit_ada_compliance(html: &str) -> AdaCompliance {
    let lower = html.to_lowercase();
    let has_skip =
        lower.contains("skip to") || lower.contains("skip-to") || lower.contains("#main");
    let has_aria = lower.contains("role=")
        && (lower.contains("navigation")
            || lower.contains("main")
            || lower.contains("contentinfo"));
    let has_accessible_forms = lower.contains("aria-label")
        || lower.contains("aria-labelledby")
        || lower.contains("<label");
    let has_transcripts = lower.contains("transcript") || lower.contains("transcription");
    let has_captions = lower.contains("caption") || lower.contains("cc");
    let keyboard = lower.contains("tabindex") || lower.contains("accesskey");

    let mut score = 0u8;
    if has_skip {
        score = score.saturating_add(20);
    }
    if has_aria {
        score = score.saturating_add(20);
    }
    if has_accessible_forms {
        score = score.saturating_add(15);
    }
    if has_transcripts {
        score = score.saturating_add(15);
    }
    if has_captions {
        score = score.saturating_add(15);
    }
    if keyboard {
        score = score.saturating_add(15);
    }

    AdaCompliance {
        has_skip_links: has_skip,
        has_aria_landmarks: has_aria,
        has_accessible_forms: has_accessible_forms,
        has_transcripts,
        has_captions,
        keyboard_navigable: keyboard,
        score,
    }
}
