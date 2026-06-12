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

/// Image alt audit.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ImageAltAudit {
    pub total_images: usize,
    pub images_with_alt: usize,
    pub images_without_alt: usize,
    pub decorative_images: usize,
    pub score: u8,
}

/// Audit image alt tags.
#[must_use]
pub fn audit_image_alt(html: &str) -> ImageAltAudit {
    let dom = parse_html(html);
    audit_image_alt_from_dom(&dom)
}

/// Audit image alt tags from an existing DOM.
pub fn audit_image_alt_from_dom(dom: &RcDom) -> ImageAltAudit {
    let mut total: usize = 0;
    let mut with_alt: usize = 0;
    let mut decorative = 0;

    audit_img_node(&dom.document, &mut total, &mut with_alt, &mut decorative);

    let without = total.saturating_sub(with_alt);
    let score = if total == 0 {
        100
    } else {
        ((with_alt as f32 / total as f32) * 100.0) as u8
    };
    ImageAltAudit {
        total_images: total,
        images_with_alt: with_alt,
        images_without_alt: without,
        decorative_images: decorative,
        score,
    }
}

fn audit_img_node(node: &Handle, total: &mut usize, with_alt: &mut usize, decorative: &mut usize) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "img" {
                *total += 1;
                let attrs = attrs.borrow();
                for attr in attrs.iter() {
                    if attr.name.local.as_ref() == "alt" {
                        *with_alt += 1;
                        let val = attr.value.trim();
                        if val.is_empty() {
                            *decorative += 1;
                        }
                        break;
                    }
                }
            }
            for child in node.children.borrow().iter() {
                audit_img_node(child, total, with_alt, decorative);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                audit_img_node(child, total, with_alt, decorative);
            }
        }
    }
}
