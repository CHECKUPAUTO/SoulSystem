use crate::extractor::{parse_html, Handle, NodeData, RcDom};

/// Extract the `lang` attribute from the first `<html>` tag.
#[must_use]
pub fn extract_language(html: &str) -> Option<String> {
    let dom = parse_html(html);
    extract_language_from_dom(&dom)
}

/// Extract the `lang` attribute from an existing DOM.
pub fn extract_language_from_dom(dom: &RcDom) -> Option<String> {
    extract_lang_from_node(&dom.document)
}

fn extract_lang_from_node(node: &Handle) -> Option<String> {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "html" {
                let attrs = attrs.borrow();
                for attr in attrs.iter() {
                    if attr.name.local.as_ref() == "lang" {
                        return Some(attr.value.to_string().to_lowercase());
                    }
                }
            }
            for child in node.children.borrow().iter() {
                if let Some(lang) = extract_lang_from_node(child) {
                    return Some(lang);
                }
            }
            None
        }
        _ => {
            for child in node.children.borrow().iter() {
                if let Some(lang) = extract_lang_from_node(child) {
                    return Some(lang);
                }
            }
            None
        }
    }
}
