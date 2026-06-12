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

use crate::extractor::{parse_html, Handle, NodeData, RcDom};

/// Heading.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

/// Heading outline.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HeadingOutline {
    pub headings: Vec<Heading>,
    pub h1_count: usize,
    pub h2_count: usize,
    pub h3_count: usize,
    pub has_h1: bool,
    pub outline_valid: bool,
}

/// Extract heading outline.
#[must_use]
pub fn extract_headings(html: &str) -> HeadingOutline {
    let dom = parse_html(html);
    extract_headings_from_dom(&dom)
}

/// Extract heading outline from an existing DOM.
pub fn extract_headings_from_dom(dom: &RcDom) -> HeadingOutline {
    let mut headings = Vec::new();
    let mut h1 = 0;
    let mut h2 = 0;
    let mut h3 = 0;
    let mut last_level = 0u8;
    let mut valid = true;

    extract_headings_node(
        &dom.document,
        &mut headings,
        &mut last_level,
        &mut valid,
        &mut h1,
        &mut h2,
        &mut h3,
    );

    HeadingOutline {
        headings,
        h1_count: h1,
        h2_count: h2,
        h3_count: h3,
        has_h1: h1 > 0,
        outline_valid: valid,
    }
}

fn extract_headings_node(
    node: &Handle,
    headings: &mut Vec<Heading>,
    last_level: &mut u8,
    valid: &mut bool,
    h1: &mut usize,
    h2: &mut usize,
    h3: &mut usize,
) {
    match &node.data {
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if tag.len() == 2 && tag.starts_with('h') {
                if let Ok(level) = tag[1..].parse::<u8>() {
                    if (1..=6).contains(&level) {
                        let text = collect_text(node).trim().to_string();
                        if level > *last_level + 1 && *last_level > 0 {
                            *valid = false;
                        }
                        *last_level = level;
                        headings.push(Heading { level, text });
                        match level {
                            1 => *h1 += 1,
                            2 => *h2 += 1,
                            3 => *h3 += 1,
                            _ => {}
                        }
                    }
                }
            }
            for child in node.children.borrow().iter() {
                extract_headings_node(child, headings, last_level, valid, h1, h2, h3);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_headings_node(child, headings, last_level, valid, h1, h2, h3);
            }
        }
    }
}

fn collect_text(node: &Handle) -> String {
    let mut text = String::new();
    match &node.data {
        NodeData::Text { contents } => text.push_str(&contents.borrow()),
        _ => {
            for child in node.children.borrow().iter() {
                text.push_str(&collect_text(child));
            }
        }
    }
    text
}
