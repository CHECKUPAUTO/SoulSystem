use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Enriched page metadata extracted from HTML.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PageMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
    pub canonical_url: Option<String>,
}

/// Extract metadata from raw HTML.
#[must_use]
pub fn extract_metadata(html: &str) -> PageMetadata {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");

    extract_metadata_from_dom(&dom)
}

/// Extract metadata from an existing DOM.
pub fn extract_metadata_from_dom(dom: &RcDom) -> PageMetadata {
    let mut meta = PageMetadata::default();
    extract_meta_from_node(&dom.document, &mut meta);
    meta
}

fn extract_meta_from_node(node: &Handle, meta: &mut PageMetadata) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "title" && meta.title.is_none() {
                meta.title = extract_text_children(node);
            }
            if name.local.as_ref() == "meta" {
                let attrs = attrs.borrow();
                let mut name_attr = None;
                let mut property_attr = None;
                let mut content_attr = None;
                for attr in attrs.iter() {
                    match attr.name.local.as_ref() {
                        "name" => name_attr = Some(attr.value.to_string()),
                        "property" => property_attr = Some(attr.value.to_string()),
                        "content" => content_attr = Some(attr.value.to_string()),
                        _ => {}
                    }
                }
                if let Some(content) = content_attr {
                    if let Some(ref name_val) = name_attr {
                        if name_val.eq_ignore_ascii_case("description") {
                            meta.description = Some(content.clone());
                        }
                    }
                    if let Some(ref prop) = property_attr {
                        if prop.eq_ignore_ascii_case("og:title") {
                            meta.og_title = Some(content.clone());
                        } else if prop.eq_ignore_ascii_case("og:description") {
                            meta.og_description = Some(content.clone());
                        } else if prop.eq_ignore_ascii_case("og:image") {
                            meta.og_image = Some(content.clone());
                        }
                    }
                }
            }
            if name.local.as_ref() == "link" {
                let attrs = attrs.borrow();
                let mut rel = None;
                let mut href = None;
                for attr in attrs.iter() {
                    match attr.name.local.as_ref() {
                        "rel" => rel = Some(attr.value.to_string()),
                        "href" => href = Some(attr.value.to_string()),
                        _ => {}
                    }
                }
                if let (Some(r), Some(h)) = (rel, href) {
                    if r.eq_ignore_ascii_case("canonical") {
                        meta.canonical_url = Some(h);
                    }
                }
            }
            for child in node.children.borrow().iter() {
                extract_meta_from_node(child, meta);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_meta_from_node(child, meta);
            }
        }
    }
}

fn extract_text_children(node: &Handle) -> Option<String> {
    let mut text = String::new();
    for child in node.children.borrow().iter() {
        if let NodeData::Text { contents } = &child.data {
            text.push_str(&contents.borrow());
        }
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
