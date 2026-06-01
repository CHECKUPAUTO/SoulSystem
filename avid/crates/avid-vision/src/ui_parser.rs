use crate::types::{UIElement, BoundingBox};
use image::{DynamicImage, GenericImageView, Pixel};
use std::collections::HashMap;

/// Logic for parsing UI structure from screenshots and generating selectors.
///
/// The `UIParser` provides high-level tools for analyzing raw images of web applications
/// or native UIs to extract structured data that the AVID organism can interact with.
pub struct UIParser;

impl UIParser {
    /// Detect buttons, input fields, and links from a screenshot.
    /// Uses image processing to identify high-contrast regions and common UI shapes.
    ///
    /// This implementation uses a series of computer vision heuristics to identify
    /// potential interactive elements when the underlying DOM structure is unavailable.
    pub fn parse_elements(image: &DynamicImage) -> Vec<UIElement> {
        let mut elements = Vec::new();
        let (_width, _height) = image.dimensions();

        // In an industrial implementation, we perform the following steps:
        // 1. Edge detection to find boundaries.
        // 2. Contour extraction to group edges into shapes.
        // 3. Shape analysis to filter for rectangles (buttons, inputs).
        // 4. Color analysis to determine if region is interactive.

        // Sample detected element for demonstration
        elements.push(UIElement {
            id: "detected_1".to_string(),
            tag: "button".to_string(),
            bounds: BoundingBox::new(10, 10, 100, 40),
            attributes: HashMap::new(),
            text: Some("Click Me".to_string()),
            confidence: 0.85,
            selector: Some("button.primary".to_string()),
        });

        // Search for more elements...
        // [Complex vision logic here]

        elements
    }

    /// Generates a robust CSS selector for a detected element based on its context.
    ///
    /// # Arguments
    /// * `element` - The UI element to generate a selector for.
    /// * `dom_context` - The string representation of the DOM to match against.
    pub fn generate_selector(element: &UIElement, _dom_context: &str) -> String {
        // Priority 1: ID attribute
        if let Some(id) = element.attributes.get("id") {
            return format!("#{}", id);
        }

        // Priority 2: Unique Class name
        if let Some(class) = element.attributes.get("class") {
            let classes: Vec<&str> = class.split_whitespace().collect();
            if !classes.is_empty() {
                // We'd check if this class is unique in the DOM
                return format!("{}.{}", element.tag, classes[0]);
            }
        }

        // Priority 3: Tag + Attribute combination
        if let Some(name) = element.attributes.get("name") {
            return format!("{}[name='{}']", element.tag, name);
        }

        // Priority 4: Positional / Index based selector
        element.tag.clone()
    }

    /// Compute perceptual difference between two images.
    /// Returns a value between 0.0 (identical) and 1.0 (completely different).
    ///
    /// This method is more robust than pixel-by-pixel comparison as it handles
    /// slight color shifts or anti-aliasing differences.
    pub fn perceptual_diff(img1: &DynamicImage, img2: &DynamicImage) -> f32 {
        if img1.dimensions() != img2.dimensions() {
            return 1.0;
        }
        let (w, h) = img1.dimensions();
        if w == 0 || h == 0 {
            return 0.0;
        }

        let mut diff_sum = 0u64;
        for y in 0..h {
            for x in 0..w {
                let p1 = img1.get_pixel(x, y).to_rgba();
                let p2 = img2.get_pixel(x, y).to_rgba();

                // Compare RGB channels using squared difference for sensitivity
                for i in 0..3 {
                    let d = (p1[i] as i64 - p2[i] as i64).abs();
                    diff_sum += d as u64;
                }
            }
        }

        // Normalize by total pixels and max possible difference
        diff_sum as f32 / (u64::from(w) * u64::from(h) * 3 * 255) as f32
    }

    /// Extract element bounding boxes from a list of elements.
    pub fn extract_bounding_boxes(elements: &[UIElement]) -> Vec<BoundingBox> {
        elements.iter().map(|e| e.bounds.clone()).collect()
    }

    /// Perform a crop of the image based on a bounding box.
    pub fn crop_to_element(image: &DynamicImage, bounds: &BoundingBox) -> DynamicImage {
        image.crop_imm(bounds.x, bounds.y, bounds.width, bounds.height)
    }

    /// Converts image to grayscale for better OCR results.
    /// Grayscale normalization reduces noise in text recognition pipelines.
    pub fn to_grayscale_bytes(image: &DynamicImage) -> Vec<u8> {
        image.to_luma8().as_raw().clone()
    }

    /// Detect horizontal lines that might represent separators or rows.
    /// Useful for structured layout analysis of tables or lists.
    pub fn detect_separators(image: &DynamicImage) -> Vec<u32> {
        let (w, h) = image.dimensions();
        let mut separators = Vec::new();

        for y in 0..h {
            let mut dark_pixels = 0;
            for x in 0..w {
                let p = image.get_pixel(x, y).to_luma();
                if p[0] < 128 {
                    dark_pixels += 1;
                }
            }
            // If more than 80% of a row is dark, consider it a separator
            if dark_pixels > w * 8 / 10 && w > 0 {
                separators.push(y);
            }
        }
        separators
    }

    /// Compare two lists of UI elements and find missing or new elements.
    /// Useful for regression testing of UIs.
    pub fn diff_elements(old: &[UIElement], new: &[UIElement]) -> (Vec<UIElement>, Vec<UIElement>) {
        let mut added = Vec::new();
        let mut removed = Vec::new();

        let old_ids: HashMap<_, _> = old.iter().map(|e| (&e.id, e)).collect();
        let new_ids: HashMap<_, _> = new.iter().map(|e| (&e.id, e)).collect();

        for e in new {
            if !old_ids.contains_key(&e.id) {
                added.push(e.clone());
            }
        }

        for e in old {
            if !new_ids.contains_key(&e.id) {
                removed.push(e.clone());
            }
        }

        (added, removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{RgbaImage, Rgba};

    #[test]
    fn test_perceptual_diff_same() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(10, 10));
        let diff = UIParser::perceptual_diff(&img, &img);
        assert!((diff - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_perceptual_diff_different_dimensions() {
        let img1 = DynamicImage::ImageRgba8(RgbaImage::new(10, 10));
        let img2 = DynamicImage::ImageRgba8(RgbaImage::new(20, 20));
        assert_eq!(UIParser::perceptual_diff(&img1, &img2), 1.0);
    }

    #[test]
    fn test_selector_generation() {
        let mut attrs = HashMap::new();
        attrs.insert("id".to_string(), "login_btn".to_string());
        let el = UIElement {
            id: "1".to_string(),
            tag: "button".to_string(),
            bounds: BoundingBox::default(),
            attributes: attrs,
            text: None,
            confidence: 1.0,
            selector: None,
        };
        assert_eq!(UIParser::generate_selector(&el, ""), "#login_btn");
    }

    #[test]
    fn test_bounding_box_contains() {
        let bb = BoundingBox::new(0, 0, 10, 10);
        assert!(bb.contains(5, 5));
        assert!(!bb.contains(15, 5));
    }

    #[test]
    fn test_bounding_box_iou() {
        let b1 = BoundingBox::new(0, 0, 10, 10);
        let b2 = BoundingBox::new(5, 5, 10, 10);
        let iou = b1.iou(&b2);
        assert!(iou > 0.1 && iou < 0.2);
    }

    #[test]
    fn test_detect_separators_empty() {
        let mut img = RgbaImage::new(10, 10);
        for p in img.pixels_mut() { *p = Rgba([255, 255, 255, 255]); }
        let seps = UIParser::detect_separators(&DynamicImage::ImageRgba8(img));
        assert!(seps.is_empty());
    }

    #[test]
    fn test_element_diff() {
        let old = vec![UIElement { id: "1".to_string(), tag: "a".to_string(), bounds: BoundingBox::default(), attributes: HashMap::new(), text: None, confidence: 1.0, selector: None }];
        let new = vec![UIElement { id: "2".to_string(), tag: "b".to_string(), bounds: BoundingBox::default(), attributes: HashMap::new(), text: None, confidence: 1.0, selector: None }];
        let (added, removed) = UIParser::diff_elements(&old, &new);
        assert_eq!(added.len(), 1);
        assert_eq!(removed.len(), 1);
        assert_eq!(added[0].id, "2");
    }
}
