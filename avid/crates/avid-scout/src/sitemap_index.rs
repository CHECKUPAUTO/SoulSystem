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

/// Sitemap index entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SitemapIndexEntry {
    pub loc: String,
    pub lastmod: Option<String>,
}

/// Parse a sitemap index XML.
#[must_use]
pub fn parse_sitemap_index(xml: &str) -> Vec<SitemapIndexEntry> {
    let mut entries = Vec::new();
    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<loc>") && trimmed.ends_with("</loc>") {
            let loc = trimmed[5..trimmed.len() - 6].to_string();
            entries.push(SitemapIndexEntry { loc, lastmod: None });
        } else if trimmed.starts_with("<lastmod>") && trimmed.ends_with("</lastmod>") {
            if let Some(last) = entries.last_mut() {
                last.lastmod = Some(trimmed[9..trimmed.len() - 10].to_string());
            }
        }
    }
    entries
}
