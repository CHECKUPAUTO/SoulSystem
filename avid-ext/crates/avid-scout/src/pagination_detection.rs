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

/// Pagination info.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Pagination {
    pub has_pagination: bool,
    pub current_page: Option<usize>,
    pub total_pages: Option<usize>,
    pub next_url: Option<String>,
    pub prev_url: Option<String>,
    pub rel_next: bool,
    pub rel_prev: bool,
}

fn page_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)page[=_-]?(\d+)").unwrap())
}

/// Detect pagination signals.
#[must_use]
pub fn detect_pagination(html: &str, links: &[impl AsRef<str>]) -> Pagination {
    let lower = html.to_lowercase();
    let mut current = None;
    let mut total = None;
    let mut next = None;
    let mut prev = None;
    let mut rel_next = false;
    let mut rel_prev = false;

    for cap in page_regex().captures_iter(html) {
        if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) {
            if current.map_or(true, |c| c < n) {
                total = Some(n.max(total.unwrap_or(0)));
            }
        }
    }

    for link in links {
        let l = link.as_ref().to_lowercase();
        if l.contains("rel=\"next\"")
            || l.contains("rel=next")
            || l.contains("next page")
            || l.contains("page suivante")
        {
            next = Some(link.as_ref().to_string());
            rel_next = true;
        }
        if l.contains("rel=\"prev\"")
            || l.contains("rel=prev")
            || l.contains("previous page")
            || l.contains("page precedente")
        {
            prev = Some(link.as_ref().to_string());
            rel_prev = true;
        }
    }

    if lower.contains("pagination") || lower.contains("paginator") || rel_next || rel_prev {
        current = Some(1);
    }

    Pagination {
        has_pagination: rel_next
            || rel_prev
            || lower.contains("pagination")
            || lower.contains("paginator"),
        current_page: current,
        total_pages: total,
        next_url: next,
        prev_url: prev,
        rel_next,
        rel_prev,
    }
}
