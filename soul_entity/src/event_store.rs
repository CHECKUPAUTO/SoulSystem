use soul_gateway::EntityEvent;
use soul_journal::RotatingJournal;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Persistent event store using soul_journal (mmap + rotation)
pub struct PersistentEventStore {
    journal: Arc<Mutex<RotatingJournal>>,
    recent_cache: Arc<Mutex<Vec<EntityEvent>>>,
    cache_capacity: usize,
}

impl PersistentEventStore {
    pub fn new<P: AsRef<Path>>(
        path: P,
        segment_size_mb: usize,
        cache_capacity: usize,
    ) -> std::io::Result<Self> {
        let path_str = path.as_ref().to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "chemin non UTF-8")
        })?;
        let journal = RotatingJournal::new_with_size(path_str, segment_size_mb * 1024 * 1024)?;
        Ok(Self {
            journal: Arc::new(Mutex::new(journal)),
            recent_cache: Arc::new(Mutex::new(Vec::with_capacity(cache_capacity))),
            cache_capacity,
        })
    }

    pub fn publish(&self, event: EntityEvent) {
        let json = serde_json::to_vec(&event).unwrap_or_default();
        let tag = event_tag(&event);

        let journal = self.journal.lock().unwrap_or_else(|e| e.into_inner());
        journal.append_log(tag, &json);

        let mut cache = self.recent_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.push(event);
        let len = cache.len();
        if len > self.cache_capacity {
            cache.drain(0..len - self.cache_capacity);
        }
    }

    pub fn recent(&self, n: usize) -> Vec<EntityEvent> {
        let cache = self.recent_cache.lock().unwrap_or_else(|e| e.into_inner());
        let start = cache.len().saturating_sub(n);
        cache[start..].to_vec()
    }
}

fn event_tag(event: &EntityEvent) -> u32 {
    match event {
        EntityEvent::GoalCreated { .. } => 1,
        EntityEvent::PlanCreated { .. } => 2,
        EntityEvent::StepExecuted { .. } => 3,
        EntityEvent::Observation { .. } => 4,
        EntityEvent::Decision { .. } => 5,
        EntityEvent::CycleStarted { .. } => 6,
        EntityEvent::CycleFinished { .. } => 7,
        EntityEvent::Error { .. } => 8,
        EntityEvent::Heartbeat { .. } => 9,
    }
}
