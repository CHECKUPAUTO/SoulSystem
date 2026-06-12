use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a bounding box in 2D space.
/// Used for locating UI elements and detected objects in screenshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BoundingBox {
    /// X coordinate of the top-left corner.
    pub x: u32,
    /// Y coordinate of the top-left corner.
    pub y: u32,
    /// Width of the box in pixels.
    pub width: u32,
    /// Height of the box in pixels.
    pub height: u32,
}

impl BoundingBox {
    /// Create a new bounding box.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if a point (px, py) is inside the bounding box.
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    /// Calculate the area of the bounding box.
    pub fn area(&self) -> u32 {
        self.width * self.height
    }

    /// Calculate Intersection over Union (IoU) with another box.
    pub fn iou(&self, other: &Self) -> f32 {
        let x_left = self.x.max(other.x);
        let y_top = self.y.max(other.y);
        let x_right = (self.x + self.width).min(other.x + other.width);
        let y_bottom = (self.y + self.height).min(other.y + other.height);

        if x_right < x_left || y_bottom < y_top {
            return 0.0;
        }

        let intersection_area = (x_right - x_left) * (y_bottom - y_top);
        let union_area = self.area() + other.area() - intersection_area;
        intersection_area as f32 / union_area as f32
    }
}

/// A detected UI element with its properties and location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElement {
    /// Unique identifier for the element within the page.
    pub id: String,
    /// HTML tag name (e.g., "button", "input").
    pub tag: String,
    /// Visual bounds of the element.
    pub bounds: BoundingBox,
    /// Dictionary of HTML attributes (e.g., class, name, type).
    pub attributes: HashMap<String, String>,
    /// Inner text of the element, if any.
    pub text: Option<String>,
    /// Confidence score of the detection (0.0 to 1.0).
    pub confidence: f32,
    /// CSS selector that uniquely identifies this element.
    pub selector: Option<String>,
}

/// Descriptive analysis of a visual scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDescription {
    /// Natural language caption describing the image content.
    pub caption: String,
    /// List of descriptive tags or labels.
    pub tags: Vec<String>,
    /// Confidence score for the overall scene analysis.
    pub confidence: f32,
}

/// Complete result of a vision engine analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResult {
    /// List of all UI elements detected in the screenshot.
    pub elements: Vec<UIElement>,
    /// Overall scene description, if requested.
    pub scene: Option<SceneDescription>,
    /// Timestamp when the analysis was performed.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Dimensions of the processed image.
    pub dimensions: (u32, u32),
}

impl VisionResult {
    /// Filter elements by tag name.
    pub fn find_by_tag(&self, tag: &str) -> Vec<&UIElement> {
        self.elements.iter().filter(|e| e.tag == tag).collect()
    }

    /// Find the element at a specific pixel location.
    pub fn element_at(&self, x: u32, y: u32) -> Option<&UIElement> {
        self.elements
            .iter()
            .filter(|e| e.bounds.contains(x, y))
            .min_by_key(|e| e.bounds.area()) // Prefer the smallest (deepest) element
    }
}
