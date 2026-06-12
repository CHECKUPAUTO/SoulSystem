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

/// Feed type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FeedType {
    Rss,
    Atom,
    JsonFeed,
}

/// Feed reference.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeedRef {
    pub url: String,
    pub feed_type: FeedType,
    pub title: Option<String>,
}

/// Detect RSS/Atom feed links in HTML.
#[must_use]
pub fn detect_feeds(html: &str) -> Vec<FeedRef> {
    let mut feeds = Vec::new();
    let lower = html.to_lowercase();
    for line in html.lines() {
        let l = line.to_lowercase();
        if l.contains("<link") && l.contains("alternate") {
            if l.contains("application/rss+xml") || l.contains("type=\"rss\"") {
                let url = extract_href(line);
                feeds.push(FeedRef {
                    url,
                    feed_type: FeedType::Rss,
                    title: extract_title_attr(line),
                });
            } else if l.contains("application/atom+xml") || l.contains("type=\"atom\"") {
                let url = extract_href(line);
                feeds.push(FeedRef {
                    url,
                    feed_type: FeedType::Atom,
                    title: extract_title_attr(line),
                });
            } else if l.contains("application/json") && l.contains("feed") {
                let url = extract_href(line);
                feeds.push(FeedRef {
                    url,
                    feed_type: FeedType::JsonFeed,
                    title: extract_title_attr(line),
                });
            }
        }
    }
    if lower.contains("/feed") && !feeds.iter().any(|f| f.url.contains("/feed")) {
        feeds.push(FeedRef {
            url: "/feed".to_string(),
            feed_type: FeedType::Rss,
            title: None,
        });
    }
    if lower.contains("/rss") && !feeds.iter().any(|f| f.url.contains("/rss")) {
        feeds.push(FeedRef {
            url: "/rss".to_string(),
            feed_type: FeedType::Rss,
            title: None,
        });
    }
    feeds
}

fn extract_href(line: &str) -> String {
    if let Some(start) = line.find("href=") {
        let rest = &line[start + 5..];
        let delim = rest.chars().next().unwrap_or('\u{0026}');
        if delim == '\"' || delim == '\u{0026}' {
            if let Some(end) = rest[1..].find(delim) {
                return rest[1..end + 1].to_string();
            }
        } else {
            if let Some(end) = rest.find(' ') {
                return rest[..end].to_string();
            }
        }
    }
    String::new()
}

fn extract_title_attr(line: &str) -> Option<String> {
    if let Some(start) = line.find("title=") {
        let rest = &line[start + 6..];
        let delim = rest.chars().next().unwrap_or('\u{0026}');
        if delim == '\"' || delim == '\u{0026}' {
            if let Some(end) = rest[1..].find(delim) {
                return Some(rest[1..end + 1].to_string());
            }
        }
    }
    None
}
