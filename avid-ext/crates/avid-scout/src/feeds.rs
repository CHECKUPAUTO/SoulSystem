#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use quick_xml::events::Event;
use quick_xml::Reader;

/// An RSS/Atom feed entry.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FeedEntry {
    pub title: String,
    pub link: String,
    pub description: Option<String>,
    pub published: Option<String>,
}

/// Parse RSS or Atom XML.
#[must_use]
pub fn parse_feed(xml: &str) -> Vec<FeedEntry> {
    let lower = xml.to_lowercase();
    if lower.contains("<feed") {
        parse_atom(xml)
    } else {
        parse_rss(xml)
    }
}

fn parse_rss(xml: &str) -> Vec<FeedEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut entries = Vec::new();
    let mut current = FeedEntry::default();
    let mut in_item = false;
    let mut current_tag = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                if name == "item" {
                    in_item = true;
                    current = FeedEntry::default();
                }
                current_tag = name;
            }
            Ok(Event::Text(e)) if in_item => {
                let text = e.unescape().unwrap_or_default().into_owned();
                match current_tag.as_str() {
                    "title" => current.title = text,
                    "link" => current.link = text,
                    "description" => current.description = Some(text),
                    "pubdate" => current.published = Some(text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                if name == "item" {
                    entries.push(current.clone());
                    in_item = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    entries
}

fn parse_atom(xml: &str) -> Vec<FeedEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut entries = Vec::new();
    let mut current = FeedEntry::default();
    let mut in_entry = false;
    let mut current_tag = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                if name == "entry" {
                    in_entry = true;
                    current = FeedEntry::default();
                }
                current_tag = name;
            }
            Ok(Event::Text(e)) if in_entry => {
                let text = e.unescape().unwrap_or_default().into_owned();
                match current_tag.as_str() {
                    "title" => current.title = text,
                    "id" if current.link.is_empty() => current.link = text,
                    "summary" => current.description = Some(text),
                    "updated" => current.published = Some(text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                if name == "entry" {
                    entries.push(current.clone());
                    in_entry = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    entries
}
