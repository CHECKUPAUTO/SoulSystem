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

/// A form field.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormField {
    pub name: String,
    pub field_type: String,
    pub value: Option<String>,
    pub required: bool,
}

/// A parsed HTML form.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormInfo {
    pub action: Option<String>,
    pub method: String,
    pub fields: Vec<FormField>,
}

/// Extract all forms from raw HTML.
#[must_use]
pub fn extract_forms(html: &str) -> Vec<FormInfo> {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");
    extract_forms_from_dom(&dom)
}

/// Extract all forms from an existing DOM.
pub fn extract_forms_from_dom(dom: &RcDom) -> Vec<FormInfo> {
    let mut forms = Vec::new();
    extract_forms_from_node(&dom.document, &mut forms);
    forms
}

fn extract_forms_from_node(node: &Handle, forms: &mut Vec<FormInfo>) {
    match &node.data {
        NodeData::Element { name, attrs, .. } if name.local.as_ref() == "form" => {
            let attrs = attrs.borrow();
            let mut action = None;
            let mut method = "get".to_string();
            for attr in attrs.iter() {
                match attr.name.local.as_ref() {
                    "action" => action = Some(attr.value.to_string()),
                    "method" => method = attr.value.to_lowercase(),
                    _ => {}
                }
            }
            let mut fields = Vec::new();
            for child in node.children.borrow().iter() {
                extract_fields(child, &mut fields);
            }
            forms.push(FormInfo {
                action,
                method,
                fields,
            });
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_forms_from_node(child, forms);
            }
        }
    }
}

fn extract_fields(node: &Handle, fields: &mut Vec<FormField>) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "input"
                || name.local.as_ref() == "textarea"
                || name.local.as_ref() == "select"
            {
                let attrs = attrs.borrow();
                let mut name_attr = String::new();
                let mut field_type = name.local.as_ref().to_string();
                let mut value = None;
                let mut required = false;
                for attr in attrs.iter() {
                    match attr.name.local.as_ref() {
                        "name" => name_attr = attr.value.to_string(),
                        "type" => field_type = attr.value.to_string(),
                        "value" => value = Some(attr.value.to_string()),
                        "required" => required = true,
                        _ => {}
                    }
                }
                fields.push(FormField {
                    name: name_attr,
                    field_type,
                    value,
                    required,
                });
            }
            for child in node.children.borrow().iter() {
                extract_fields(child, fields);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_fields(child, fields);
            }
        }
    }
}
