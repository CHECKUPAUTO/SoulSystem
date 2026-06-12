use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
pub use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Parse HTML into an RcDom.
pub fn parse_html(html: &str) -> RcDom {
    let opts = ParseOpts::default();
    parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string")
}

/// Extract plain text from HTML, skipping `script` and `style` content.
pub fn extract_text(html: &str) -> String {
    let dom = parse_html(html);
    extract_text_from_dom(&dom)
}

/// Extract plain text from an existing DOM.
pub fn extract_text_from_dom(dom: &RcDom) -> String {
    let mut text = String::new();
    extract_text_from_node(&dom.document, &mut text);
    text
}

fn extract_text_from_node(node: &Handle, text: &mut String) {
    match &node.data {
        NodeData::Text { contents } => {
            text.push_str(&contents.borrow());
        }
        NodeData::Element { name, .. } => {
            if name.local.as_ref() == "script" || name.local.as_ref() == "style" {
                return;
            }
            for child in node.children.borrow().iter() {
                extract_text_from_node(child, text);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_text_from_node(child, text);
            }
        }
    }
}

/// Extract the contents of the first `<title>` tag.
pub fn extract_title(html: &str) -> Option<String> {
    let dom = parse_html(html);
    extract_title_from_dom(&dom)
}

/// Extract the title from an existing DOM.
pub fn extract_title_from_dom(dom: &RcDom) -> Option<String> {
    extract_title_from_node(&dom.document)
}

fn extract_title_from_node(node: &Handle) -> Option<String> {
    match &node.data {
        NodeData::Element { name, .. } if name.local.as_ref() == "title" => {
            let mut title = String::new();
            for child in node.children.borrow().iter() {
                if let NodeData::Text { contents } = &child.data {
                    title.push_str(&contents.borrow());
                }
            }
            let trimmed = title.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        NodeData::Element { .. } | NodeData::Document => {
            for child in node.children.borrow().iter() {
                if let Some(title) = extract_title_from_node(child) {
                    return Some(title);
                }
            }
            None
        }
        _ => None,
    }
}

/// Extract all `href` attributes from anchor tags.
pub fn extract_links(html: &str) -> Vec<String> {
    let dom = parse_html(html);
    extract_links_from_dom(&dom)
}

/// Extract links from an existing DOM.
pub fn extract_links_from_dom(dom: &RcDom) -> Vec<String> {
    let mut links = Vec::new();
    extract_links_from_node(&dom.document, &mut links);
    links
}

fn extract_links_from_node(node: &Handle, links: &mut Vec<String>) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "a" {
                for attr in attrs.borrow().iter() {
                    if attr.name.local.as_ref() == "href" {
                        links.push(attr.value.to_string());
                    }
                }
            }
            for child in node.children.borrow().iter() {
                extract_links_from_node(child, links);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_links_from_node(child, links);
            }
        }
    }
}
