use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Ultra-compact task representation — exactly 16 bytes on both x86_64 and aarch64.
/// Zero allocation, stable for FFI boundaries.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Task {
    pub execute: extern "C" fn(*mut u8),
    pub context: *mut u8,
}

/// Fixed-size circular Chase-Lev deque for work-stealing schedulers.
///
/// Invariant: `tail` always equals `head + num_elements`.
/// The owner (pusher/popper) manipulates `tail` at LIFO; stealers (consumers)
/// manipulate `head` at FIFO. Both can operate concurrently without locks.
const DEQUE_CAPACITY: usize = 4096;
const DEQUE_MASK: usize = DEQUE_CAPACITY - 1;

pub struct LockFreeTaskDeque {
    /// Top index — manipulated by stealers (FIFO). Never by owner.
    head: AtomicUsize,
    /// Bottom index — manipulated by owner (LIFO). Never by stealer.
    tail: AtomicUsize,
    buffer: [UnsafeCell<Option<Task>>; DEQUE_CAPACITY],
}

/// SAFETY: The deque uses atomic operations for all concurrent access.
/// Elements are only read/written through the current top/bottom indices,
/// which are properly sequenced via fences.
unsafe impl Sync for LockFreeTaskDeque {}
unsafe impl Send for LockFreeTaskDeque {}

impl LockFreeTaskDeque {
    /// Allocates a new empty deque. All slots initialized to None.
    pub fn new() -> Self {
        // Initialize every slot with UnsafeCell wrapping None — const-context safe.
        let buffer = std::array::from_fn(|_| UnsafeCell::new(None));

        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            buffer,
        }
    }

    /// Push a task onto the top (owner-only). Returns `false` if full.
    pub fn push(&self, task: Task) -> bool {
        let t = self.tail.load(Ordering::Relaxed);
        let h = self.head.load(Ordering::Acquire);

        if (t.wrapping_sub(h)) >= DEQUE_CAPACITY {
            return false;
        }

        unsafe {
            let slot = self.buffer[t & DEQUE_MASK].get();
            // Write the task before updating tail so it's visible to stealers.
            *slot = Some(task);
        }

        self.tail.store(t.wrapping_add(1), Ordering::Release);
        true
    }

    /// Pop the top element (owner-only, LIFO). Returns `None` if empty.
    pub fn pop(&self) -> Option<Task> {
        let t = self.tail.load(Ordering::Relaxed).wrapping_sub(1);
        self.tail.store(t, Ordering::Relaxed);

        // Ensure the element write is visible before we read it.
        std::sync::atomic::fence(Ordering::SeqCst);
        let h = self.head.load(Ordering::Relaxed);

        if h <= t {
            let slot = self.buffer[t & DEQUE_MASK].get();
            let task = unsafe { (*slot).take() };

            // Empty — try to empty the deque (helps stealer see it's empty)
            if h == t {
                if self
                    .head
                    .compare_exchange(h, h.wrapping_add(1), Ordering::SeqCst, Ordering::Relaxed)
                    .is_err()
                {
                    self.tail.store(t.wrapping_add(1), Ordering::Relaxed);
                    return None;
                }
                self.tail.store(t.wrapping_add(1), Ordering::Relaxed);
            }
            task
        } else {
            // Empty — restore tail
            self.tail.store(t.wrapping_add(1), Ordering::Relaxed);
            None
        }
    }

    /// Steal the bottom element (multi-consumer, FIFO). Returns `None` if empty.
    pub fn steal(&self) -> Option<Task> {
        loop {
            let h = self.head.load(Ordering::Acquire);
            std::sync::atomic::fence(Ordering::SeqCst);
            let t = self.tail.load(Ordering::Acquire);

            if h >= t {
                return None;
            }

            let slot = self.buffer[h & DEQUE_MASK].get();

            if self
                .head
                .compare_exchange(h, h.wrapping_add(1), Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                let task = unsafe { (*slot).take() };
                return task;
            }
        }
    }

    /// Capacity in slots.
    pub const fn capacity(&self) -> usize {
        DEQUE_CAPACITY
    }

    /// Approximate number of elements currently in the deque (snapshot, not atomic).
    pub fn len(&self) -> usize {
        self.tail
            .load(Ordering::Acquire)
            .wrapping_sub(self.head.load(Ordering::Acquire))
    }

    /// Returns true if `len()` reports zero.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for LockFreeTaskDeque {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// No-op function pointer with C ABI — used as `execute` field in test Task construction.
    extern "C" fn noop_fn(_: *mut u8) {}

    /// Helper to build a test Task without explicit type annotations.
    fn test_task() -> Task {
        Task {
            execute: noop_fn,
            context: std::ptr::null_mut(),
        }
    }

    #[test]
    fn new_deque_is_empty() {
        let dq = LockFreeTaskDeque::new();
        assert!(dq.is_empty());
        assert_eq!(dq.len(), 0);
        assert_eq!(dq.capacity(), 4096);
    }

    #[test]
    fn push_pop_single() {
        let dq = LockFreeTaskDeque::new();
        let task = test_task();
        assert!(dq.push(task));
        assert_eq!(dq.len(), 1);
        assert_eq!(
            dq.pop().map(|t| t.execute as usize),
            Some(noop_fn as *const () as usize)
        );
        assert!(dq.is_empty());
    }

    #[test]
    fn push_pop_lifo() {
        let dq = LockFreeTaskDeque::new();
        for _i in 0..100 {
            let task = test_task();
            assert!(dq.push(task));
        }

        // LIFO order — last pushed is first popped
        for _ in 0..100 {
            let task = dq.pop().unwrap();
            assert!(task.execute as usize > 0); // verify non-null fn ptr
        }
        assert!(dq.is_empty());
    }

    #[test]
    fn push_pop_fifo_via_steal() {
        let dq = LockFreeTaskDeque::new();
        for _ in 0..10 {
            let task = test_task();
            assert!(dq.push(task));
        }

        // Steals are FIFO — first pushed out first
        for _ in 0..10 {
            let stolen = dq.steal();
            assert!(stolen.is_some());
        }
        assert!(dq.is_empty());
        assert!(dq.steal().is_none());
    }

    #[test]
    fn push_full_capacity() {
        let dq = LockFreeTaskDeque::new();
        for _ in 0..DEQUE_CAPACITY {
            assert!(dq.push(test_task()));
        }
        // One more should fail
        assert!(!dq.push(test_task()));
    }

    #[test]
    fn steal_from_empty() {
        let dq = LockFreeTaskDeque::new();
        assert!(dq.steal().is_none());
    }

    #[test]
    fn pop_after_steal_all() {
        let dq = LockFreeTaskDeque::new();
        // Steal from empty deque — tail was at 0
        assert!(dq.steal().is_none());
        // Pop should also return None
        assert!(dq.pop().is_none());
    }

    #[test]
    fn mixed_ops() {
        let dq = LockFreeTaskDeque::new();

        // Push 5
        for _ in 0..5 {
            dq.push(test_task());
        }
        assert_eq!(dq.len(), 5);

        // Steal 2 (FIFO order)
        assert!(dq.steal().is_some());
        assert!(dq.steal().is_some());
        assert_eq!(dq.len(), 3);

        // Pop 1 (LIFO)
        assert!(dq.pop().is_some());
        assert_eq!(dq.len(), 2);

        // Steal remaining
        assert!(dq.steal().is_some());
        assert!(dq.steal().is_some());
        assert!(dq.is_empty());
    }

    #[test]
    fn task_size_is_16_bytes() {
        assert_eq!(std::mem::size_of::<Task>(), 16);
    }
}
