//! Shared event bus connecting agent, daemon, and dashboard.
//!
//! Uses tokio broadcast channels for zero-copy event distribution.
//! Events are serialized as JSON for WebSocket forwarding.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

// ── Event Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AgentEvent {
    // Agent lifecycle
    AgentStarted {
        name: String,
        model: String,
    },
    AgentStopped {
        reason: String,
    },

    // Task execution
    TaskReceived {
        task_id: String,
        description: String,
    },
    TaskCompleted {
        task_id: String,
        result: String,
        duration_ms: u64,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },

    // ReAct loop
    Thought {
        turn: usize,
        content: String,
    },
    ToolCall {
        turn: usize,
        tool: String,
        input: String,
    },
    ToolResult {
        turn: usize,
        tool: String,
        output: String,
        success: bool,
    },
    Response {
        turn: usize,
        content: String,
    },

    // Safety
    SafetyWarning {
        message: String,
        severity: String,
    },

    // Memory
    MemoryStored {
        key: String,
        size_bytes: usize,
    },
    MemorySearched {
        query: String,
        results_count: usize,
    },

    // Dashboard
    DashboardUpdate {
        state: serde_json::Value,
    },

    // Generic
    Custom {
        topic: String,
        payload: serde_json::Value,
    },
}

// ── Event Bus ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AgentEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event
    pub fn publish(&self, event: AgentEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.sender.subscribe()
    }

    /// Truncate stored event history to `max_events` capacity.
    /// Since the underlying broadcast channel is a ring buffer, this replaces
    /// the internal sender with a new one of the requested capacity.
    /// Returns the number of events that were in the buffer before pruning.
    pub fn prune(&mut self, max_events: usize) -> usize {
        let old_len = self.sender.len();
        let (sender, _) = broadcast::channel(max_events);
        self.sender = sender;
        old_len
    }

    /// Get a cloneable handle for long-lived subscribers
    pub fn handle(&self) -> EventBusHandle {
        EventBusHandle {
            sender: self.sender.clone(),
        }
    }
}

#[derive(Clone)]
pub struct EventBusHandle {
    sender: broadcast::Sender<AgentEvent>,
}

impl EventBusHandle {
    pub fn publish(&self, event: AgentEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.sender.subscribe()
    }
}

// ── Event Forwarder (agent events → dashboard JSON) ───────────────────

pub struct EventForwarder {
    event_bus: EventBus,
    dashboard_state: Arc<tokio::sync::RwLock<serde_json::Value>>,
}

impl EventForwarder {
    pub fn new(
        event_bus: EventBus,
        dashboard_state: Arc<tokio::sync::RwLock<serde_json::Value>>,
    ) -> Self {
        Self {
            event_bus,
            dashboard_state,
        }
    }

    /// Run the forwarder (subscribes to events and updates dashboard state)
    pub async fn run(&self) {
        let mut rx = self.event_bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let json = serde_json::to_value(&event).unwrap_or_default();
                    let mut state = self.dashboard_state.write().await;
                    // Store last 100 events
                    if let Some(events) = state.get_mut("events") {
                        if let Some(arr) = events.as_array_mut() {
                            arr.push(json);
                            while arr.len() > 100 {
                                arr.remove(0);
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("EventForwarder: lagged {} events", n);
                }
                Err(_) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AgentEvent::TaskReceived {
            task_id: "t1".into(),
            description: "test task".into(),
        });

        let event = rx.recv().await.unwrap();
        match event {
            AgentEvent::TaskReceived { task_id, .. } => assert_eq!(task_id, "t1"),
            _ => panic!("unexpected event"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(AgentEvent::Thought {
            turn: 1,
            content: "thinking...".into(),
        });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        match (e1, e2) {
            (AgentEvent::Thought { content: c1, .. }, AgentEvent::Thought { content: c2, .. }) => {
                assert_eq!(c1, "thinking...");
                assert_eq!(c2, "thinking...");
            }
            _ => panic!("unexpected events"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_handle() {
        let bus = EventBus::new(16);
        let handle = bus.handle();
        let mut rx = handle.subscribe();

        handle.publish(AgentEvent::SafetyWarning {
            message: "warning".into(),
            severity: "high".into(),
        });

        let event = rx.recv().await.unwrap();
        match event {
            AgentEvent::SafetyWarning { message, .. } => assert_eq!(message, "warning"),
            _ => panic!("unexpected event"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_subscribe_dropped() {
        let bus = EventBus::new(16);
        let rx = bus.subscribe();
        drop(rx);

        bus.publish(AgentEvent::AgentStarted {
            name: "test".into(),
            model: "qwen".into(),
        });
    }

    #[tokio::test]
    async fn test_event_bus_publish_all_variants() {
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();

        bus.publish(AgentEvent::AgentStopped { reason: "done".into() });
        bus.publish(AgentEvent::TaskCompleted { task_id: "t1".into(), result: "ok".into(), duration_ms: 100 });
        bus.publish(AgentEvent::TaskFailed { task_id: "t2".into(), error: "err".into() });
        bus.publish(AgentEvent::ToolResult { turn: 1, tool: "search".into(), output: "res".into(), success: true });
        bus.publish(AgentEvent::MemoryStored { key: "k1".into(), size_bytes: 100 });
        bus.publish(AgentEvent::MemorySearched { query: "q".into(), results_count: 5 });
        bus.publish(AgentEvent::Custom { topic: "test".into(), payload: serde_json::json!({"x": 1}) });

        let mut count = 0;
        for _ in 0..7 {
            let event = rx.recv().await;
            assert!(event.is_ok());
            count += 1;
        }
        assert_eq!(count, 7);
    }

    #[tokio::test]
    async fn test_event_bus_buffer_overrun() {
        let bus = EventBus::new(2);
        let mut rx = bus.subscribe();

        bus.publish(AgentEvent::AgentStarted { name: "a".into(), model: "m".into() });
        bus.publish(AgentEvent::AgentStarted { name: "b".into(), model: "m".into() });
        bus.publish(AgentEvent::AgentStarted { name: "c".into(), model: "m".into() });

        let result = rx.recv().await;
        assert!(matches!(result, Err(broadcast::error::RecvError::Lagged(_))));
    }

    #[tokio::test]
    async fn test_event_forwarder_updates_dashboard_state() {
        let bus = EventBus::new(16);
        let state = Arc::new(tokio::sync::RwLock::new(serde_json::json!({"events": []})));
        let forwarder = EventForwarder::new(bus.clone(), state.clone());

        let handle = tokio::spawn(async move {
            forwarder.run().await;
        });

        // Give forwarder time to subscribe
        sleep(Duration::from_millis(50)).await;

        bus.publish(AgentEvent::TaskReceived {
            task_id: "t1".into(),
            description: "desc".into(),
        });

        sleep(Duration::from_millis(200)).await;

        let state_guard = state.read().await;
        let events = state_guard["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);

        handle.abort();
    }

    #[tokio::test]
    async fn test_event_forwarder_caps_at_100_events() {
        let bus = EventBus::new(256);
        let state = Arc::new(tokio::sync::RwLock::new(serde_json::json!({"events": []})));
        let forwarder = EventForwarder::new(bus.clone(), state.clone());

        let handle = tokio::spawn(async move {
            forwarder.run().await;
        });

        sleep(Duration::from_millis(50)).await;

        for i in 0..150 {
            bus.publish(AgentEvent::TaskReceived {
                task_id: format!("t{}", i),
                description: "desc".into(),
            });
        }

        sleep(Duration::from_millis(200)).await;

        let state_guard = state.read().await;
        let events = state_guard["events"].as_array().unwrap();
        assert!(events.len() <= 100);

        handle.abort();
    }
}
