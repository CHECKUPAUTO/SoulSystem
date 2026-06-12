use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// An image with its metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageInfo {
    pub src: String,
    pub alt: Option<String>,
    pub title: Option<String>,
}

/// Extract all images from HTML.
#[must_use]
pub fn extract_images(html: &str) -> Vec<ImageInfo> {
    let opts = ParseOpts::default();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever infallible from utf-8 string");

    extract_images_from_dom(&dom)
}

/// Extract all images from an existing DOM.
pub fn extract_images_from_dom(dom: &RcDom) -> Vec<ImageInfo> {
    let mut images = Vec::new();
    extract_images_from_node(&dom.document, &mut images);
    images
}

fn extract_images_from_node(node: &Handle, images: &mut Vec<ImageInfo>) {
    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            if name.local.as_ref() == "img" {
                let attrs = attrs.borrow();
                let mut src = None;
                let mut alt = None;
                let mut title = None;
                for attr in attrs.iter() {
                    match attr.name.local.as_ref() {
                        "src" => src = Some(attr.value.to_string()),
                        "alt" => alt = Some(attr.value.to_string()),
                        "title" => title = Some(attr.value.to_string()),
                        _ => {}
                    }
                }
                if let Some(s) = src {
                    images.push(ImageInfo { src: s, alt, title });
                }
            }
            for child in node.children.borrow().iter() {
                extract_images_from_node(child, images);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                extract_images_from_node(child, images);
            }
        }
    }
}
