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

/// A table row (list of cell texts).
pub type TableRow = Vec<String>;

/// A parsed HTML table with caption and rows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Table {
    pub caption: Option<String>,
    pub rows: Vec<TableRow>,
}

/// Extract all tables from raw HTML.
#[must_use]
pub fn extract_tables(html: &str) -> Vec<Table> {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");
    extract_tables_from_dom(&dom)
}

/// Extract all tables from an existing DOM.
pub fn extract_tables_from_dom(dom: &RcDom) -> Vec<Table> {
    let mut tables = Vec::new();
    extract_tables_from_node(&dom.document, &mut tables);
    tables
}

fn extract_tables_from_node(node: &Handle, tables: &mut Vec<Table>) {
    match &node.data {
        NodeData::Element { name, .. } if name.local.as_ref() == "table" => {
            let mut table = Table {
                caption: None,
                rows: Vec::new(),
            };
            for child in node.children.borrow().iter() {
                extract_table_parts(child, &mut table);
            }
            if !table.rows.is_empty() {
                tables.push(table);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_tables_from_node(child, tables);
            }
        }
    }
}

fn extract_table_parts(node: &Handle, table: &mut Table) {
    match &node.data {
        NodeData::Element { name, .. } => match name.local.as_ref() {
            "caption" => {
                table.caption = Some(collect_text(node).trim().to_string());
            }
            "tr" => {
                let mut row = Vec::new();
                for child in node.children.borrow().iter() {
                    if let NodeData::Element {
                        name: cell_name, ..
                    } = &child.data
                    {
                        if cell_name.local.as_ref() == "td" || cell_name.local.as_ref() == "th" {
                            row.push(collect_text(child).trim().to_string());
                        }
                    }
                }
                table.rows.push(row);
            }
            "thead" | "tbody" | "tfoot" => {
                for child in node.children.borrow().iter() {
                    extract_table_parts(child, table);
                }
            }
            _ => {}
        },
        _ => {}
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
