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
use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// External assets discovered in a page.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SourceLinks {
    pub css: Vec<String>,
    pub js: Vec<String>,
    pub images: Vec<String>,
    pub fonts: Vec<String>,
    pub favicon: Option<String>,
    pub apis: Vec<String>,
}

/// Extract all external asset links from HTML.
#[must_use]
pub fn extract_source_links(html: &str) -> SourceLinks {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");
    extract_source_links_from_dom(&dom)
}

/// Extract all external asset links from an existing DOM.
pub fn extract_source_links_from_dom(dom: &RcDom) -> SourceLinks {
    let mut links = SourceLinks::default();
    extract_source_links_from_node(&dom.document, &mut links);
    links
}

fn extract_source_links_from_node(node: &Handle, links: &mut SourceLinks) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            let attrs = attrs.borrow();
            match name.local.as_ref() {
                "link" => {
                    let mut rel = None;
                    let mut href = None;
                    let mut mime = None;
                    for attr in attrs.iter() {
                        match attr.name.local.as_ref() {
                            "rel" => rel = Some(attr.value.to_string()),
                            "href" => href = Some(attr.value.to_string()),
                            "type" => mime = Some(attr.value.to_string()),
                            _ => {}
                        }
                    }
                    if let Some(h) = href {
                        match rel.as_deref() {
                            Some("stylesheet") => links.css.push(h),
                            Some("icon") | Some("shortcut icon") => links.favicon = Some(h),
                            Some("preload") if mime.as_deref() == Some("font/woff2") => {
                                links.fonts.push(h);
                            }
                            _ => {}
                        }
                    }
                }
                "script" => {
                    for attr in attrs.iter() {
                        if attr.name.local.as_ref() == "src" {
                            links.js.push(attr.value.to_string());
                        }
                    }
                }
                "img" => {
                    for attr in attrs.iter() {
                        if attr.name.local.as_ref() == "src" {
                            links.images.push(attr.value.to_string());
                        }
                    }
                }
                _ => {}
            }
            for child in node.children.borrow().iter() {
                extract_source_links_from_node(child, links);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_source_links_from_node(child, links);
            }
        }
    }
}
