use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Represents a single user action captured during recording.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Action {
    /// Movement of the mouse cursor to a specific coordinate.
    MouseMove { x: f64, y: f64 },
    /// A mouse button click at a coordinate, optionally with a CSS selector.
    Click {
        x: f64,
        y: f64,
        button: String,
        /// Optional CSS selector to identify the element for robust replaying.
        selector: Option<String>
    },
    /// A scroll action with horizontal and vertical deltas.
    Scroll { delta_x: f64, delta_y: f64 },
    /// A keyboard key press.
    KeyPress { key: String },
}

/// An event recorded during a session, combining an action with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// Precise timestamp when the event occurred.
    pub timestamp: DateTime<Utc>,
    /// The action that was performed.
    pub action: Action,
    /// Optional snapshot of the DOM at the time of the action.
    pub dom_snapshot: Option<String>,
}

/// Captures and serializes user interaction sequences.
#[derive(Default)]
pub struct ActionRecorder {
    /// List of recorded events in chronological order.
    pub events: Vec<RecordedEvent>
}

impl ActionRecorder {
    /// Create a new empty action recorder.
    pub fn new() -> Self { Self::default() }

    /// Record a new action and its context.
    pub fn record(&mut self, action: Action, dom_snapshot: Option<String>) {
        self.events.push(RecordedEvent {
            timestamp: Utc::now(),
            action,
            dom_snapshot
        });
    }

    /// Serialize all recorded events to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.events)
    }

    /// Clear all recorded events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_record_action() {
        let mut recorder = ActionRecorder::new();
        recorder.record(Action::Click {
            x: 10.0,
            y: 20.0,
            button: "left".to_string(),
            selector: Some("button#submit".to_string())
        }, None);
        assert_eq!(recorder.events.len(), 1);
    }
}
