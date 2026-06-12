use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TelemetryFrame {
    pub timestamp_ns: u64,
    pub memory_throughput_bytes_per_sec: u64,
    pub active_synapse_count: u32,
    pub current_meta_loss: f32,
}

const BUFFER_CAPACITY: usize = 4096;

pub struct SystemAuditor {
    ring_buffer: Box<[TelemetryFrame; BUFFER_CAPACITY]>,
    write_index: AtomicUsize,
}

impl Default for SystemAuditor {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemAuditor {
    pub fn new() -> Self {
        let initial_frame = TelemetryFrame {
            timestamp_ns: 0,
            memory_throughput_bytes_per_sec: 0,
            active_synapse_count: 0,
            current_meta_loss: 0.0,
        };
        Self {
            ring_buffer: Box::new([initial_frame; BUFFER_CAPACITY]),
            write_index: AtomicUsize::new(0),
        }
    }

    pub fn record(&self, frame: TelemetryFrame) {
        let idx = self.write_index.fetch_add(1, Ordering::Relaxed) & (BUFFER_CAPACITY - 1);
        // Safety: We use raw pointers to avoid locks in high-frequency telemetry
        unsafe {
            let ptr = self.ring_buffer.as_ptr().add(idx) as *mut TelemetryFrame;
            *ptr = frame;
        }
    }

    pub fn get_latest(&self) -> TelemetryFrame {
        let idx =
            (self.write_index.load(Ordering::Relaxed).wrapping_sub(1)) & (BUFFER_CAPACITY - 1);
        self.ring_buffer[idx]
    }
}
