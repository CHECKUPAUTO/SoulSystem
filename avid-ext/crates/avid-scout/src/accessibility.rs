#![allow(
    clippy::cast_precision_loss,
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Accessibility audit report.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AccessibilityReport {
    pub images_without_alt: usize,
    pub forms_without_labels: usize,
    pub missing_skip_links: bool,
    pub missing_lang_attr: bool,
    pub score: u8,
}

/// Run accessibility audit on raw HTML.
#[must_use]
pub fn audit_accessibility(html: &str) -> AccessibilityReport {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");

    audit_accessibility_from_dom(&dom)
}

/// Run accessibility audit on an existing DOM.
pub fn audit_accessibility_from_dom(dom: &RcDom) -> AccessibilityReport {
    let mut report = AccessibilityReport {
        missing_skip_links: true,
        ..AccessibilityReport::default()
    };
    audit_node(&dom.document, &mut report, &mut false);

    let mut issues = 0;
    issues += report.images_without_alt;
    issues += report.forms_without_labels;
    issues += if report.missing_skip_links { 1 } else { 0 };
    issues += if report.missing_lang_attr { 1 } else { 0 };
    report.score = (100_u8).saturating_sub((issues * 10).min(100) as u8);
    report
}

fn audit_node(node: &Handle, report: &mut AccessibilityReport, has_lang: &mut bool) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "html" {
                let attrs = attrs.borrow();
                if attrs.iter().any(|a| a.name.local.as_ref() == "lang") {
                    *has_lang = true;
                } else {
                    report.missing_lang_attr = true;
                }
            }
            if name.local.as_ref() == "img" {
                let attrs = attrs.borrow();
                if !attrs.iter().any(|a| a.name.local.as_ref() == "alt") {
                    report.images_without_alt += 1;
                }
            }
            if name.local.as_ref() == "input" {
                let attrs = attrs.borrow();
                let has_label = attrs.iter().any(|a| a.name.local.as_ref() == "id");
                let has_aria = attrs.iter().any(|a| a.name.local.as_ref() == "aria-label");
                if !has_label && !has_aria {
                    report.forms_without_labels += 1;
                }
            }
            if name.local.as_ref() == "a" {
                let attrs = attrs.borrow();
                for attr in attrs.iter() {
                    if attr.name.local.as_ref() == "href"
                        && (attr.value.to_string().starts_with("#main")
                            || attr.value.to_string().starts_with("#content"))
                    {
                        report.missing_skip_links = false;
                    }
                }
            }
            for child in node.children.borrow().iter() {
                audit_node(child, report, has_lang);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                audit_node(child, report, has_lang);
            }
        }
    }
}
