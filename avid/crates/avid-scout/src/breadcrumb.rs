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

use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// A breadcrumb item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BreadcrumbItem {
    pub name: String,
    pub url: Option<String>,
}

/// Extract breadcrumbs from JSON-LD or nav HTML.
#[must_use]
pub fn extract_breadcrumbs(html: &str) -> Vec<BreadcrumbItem> {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");

    extract_breadcrumbs_from_dom(&dom)
}

/// Extract breadcrumbs from an existing DOM.
pub fn extract_breadcrumbs_from_dom(dom: &RcDom) -> Vec<BreadcrumbItem> {
    let mut items = Vec::new();
    extract_breadcrumb_nodes(&dom.document, &mut items);
    // Deduplicate consecutive identical items
    let mut deduped = Vec::new();
    for item in items {
        if deduped
            .last()
            .map_or(true, |last: &BreadcrumbItem| last.name != item.name)
        {
            deduped.push(item);
        }
    }
    deduped
}

fn extract_breadcrumb_nodes(node: &Handle, items: &mut Vec<BreadcrumbItem>) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref();
            // Look for nav, ol, ul, or elements with breadcrumb-related classes/ids
            let attrs_borrow = attrs.borrow();
            let is_breadcrumb_container = tag == "nav"
                || tag == "ol"
                || tag == "ul"
                || attrs_borrow.iter().any(|a| {
                    let val = a.value.to_lowercase();
                    val.contains("breadcrumb") || val.contains("breadcrumbs")
                });
            if is_breadcrumb_container {
                for child in node.children.borrow().iter() {
                    extract_breadcrumb_items(child, items);
                }
            } else {
                for child in node.children.borrow().iter() {
                    extract_breadcrumb_nodes(child, items);
                }
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_breadcrumb_nodes(child, items);
            }
        }
    }
}

fn extract_breadcrumb_items(node: &Handle, items: &mut Vec<BreadcrumbItem>) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref();
            if tag == "li" || tag == "a" {
                let attrs_borrow = attrs.borrow();
                let mut href = None;
                for attr in attrs_borrow.iter() {
                    if attr.name.local.as_ref() == "href" {
                        href = Some(attr.value.to_string());
                    }
                }
                let mut text = String::new();
                extract_text_from_node(node, &mut text);
                if !text.trim().is_empty() && text.len() < 100 {
                    items.push(BreadcrumbItem {
                        name: text.trim().to_string(),
                        url: href,
                    });
                }
                // Also search children for nested links in li
                for child in node.children.borrow().iter() {
                    extract_breadcrumb_items(child, items);
                }
            } else {
                for child in node.children.borrow().iter() {
                    extract_breadcrumb_items(child, items);
                }
            }
        }
        _ => {}
    }
}

fn extract_text_from_node(node: &Handle, text: &mut String) {
    match &node.data {
        NodeData::Text { contents } => {
            text.push_str(&contents.borrow());
        }
        NodeData::Element { .. } => {
            for child in node.children.borrow().iter() {
                extract_text_from_node(child, text);
            }
        }
        _ => {}
    }
}
