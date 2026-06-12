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

/// Event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub name: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub location: Option<String>,
    pub event_type: Option<String>,
}

/// Extract events from structured data.
#[must_use]
pub fn extract_events(structured: &[serde_json::Value]) -> Vec<Event> {
    let mut events = Vec::new();
    for item in structured {
        if let Some(t) = item.get("@type").and_then(|v| v.as_str()) {
            if t.eq_ignore_ascii_case("Event")
                || t.eq_ignore_ascii_case("MusicEvent")
                || t.eq_ignore_ascii_case("SportsEvent")
            {
                events.push(Event {
                    name: item.get("name").and_then(|v| v.as_str()).map(String::from),
                    start_date: item
                        .get("startDate")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    end_date: item
                        .get("endDate")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    location: item
                        .get("location")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    event_type: Some(t.to_string()),
                });
            }
        }
    }
    events
}
