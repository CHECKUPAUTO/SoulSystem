//! Bus inter-agents Lock-Free MPMC — algorithme borné de Vyukov.
//! Coordination par numéro de séquence atomique PAR CASE : aucun accès
//! non-atomique partagé, donc exempt de data race (vérifié ThreadSanitizer).
//! Payload zéro-copy (transfert de propriété du pointeur brut).

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

const IPC_QUEUE_SIZE: usize = 8192;
const IPC_MASK: usize = IPC_QUEUE_SIZE - 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AgentMessage {
    pub source_agent_id: u32,
    pub target_agent_id: u32,
    pub signal_code: u32,
    pub payload_ptr: *mut u8,
    pub payload_size: usize,
}

impl AgentMessage {
    #[inline]
    const fn empty() -> Self {
        Self {
            source_agent_id: 0,
            target_agent_id: 0,
            signal_code: 0,
            payload_ptr: std::ptr::null_mut(),
            payload_size: 0,
        }
    }
}

/// Case du ring : jeton de propreté atomique + message écrit/lu par un seul
/// thread à la fois (garanti par la séquence, pas par un verrou).
struct Cell {
    sequence: AtomicUsize,
    message: UnsafeCell<AgentMessage>,
}

/// Compteur isolé sur sa propre ligne de cache (anti-false-sharing).
#[repr(align(64))]
struct CachePad(AtomicUsize);

/// Bus MPMC lock-free borné (Vyukov).
#[repr(align(64))]
pub struct InterAgentBus {
    buffer: Box<[Cell]>,
    enqueue_pos: CachePad,
    dequeue_pos: CachePad,
}

// SAFETY: All shared access goes through atomics — the `sequence` counter acts as a
// per-cell token ensuring exactly one producer writes `message` (via UnsafeCell) and
// exactly one consumer reads it, with happens-before established by the
// Release store / Acquire load on `sequence`. The UnsafeCell itself is never aliased:
// a producer gets exclusive access via CAS on `enqueue_pos` + `sequence`, and a
// consumer via CAS on `dequeue_pos` + `sequence`. Therefore no data race exists.
// INVARIANT: `sequence` must be read with Acquire before accessing `message`, and
// `message` must not be accessed after releasing the sequence token to another party.
// FAILURE: If the sequence protocol is violated (e.g., reading message before the
// producer's Release store), a torn read could occur. This is prevented by the
// algorithm — the check `diff == 0` ensures we only touch a cell we own.
unsafe impl Sync for InterAgentBus {}
unsafe impl Send for InterAgentBus {}

impl InterAgentBus {
    pub fn new() -> Self {
        let buffer: Box<[Cell]> = (0..IPC_QUEUE_SIZE)
            .map(|i| Cell {
                sequence: AtomicUsize::new(i),
                message: UnsafeCell::new(AgentMessage::empty()),
            })
            .collect();
        Self {
            buffer,
            enqueue_pos: CachePad(AtomicUsize::new(0)),
            dequeue_pos: CachePad(AtomicUsize::new(0)),
        }
    }

    /// Publie un message (non-bloquant, lock-free). `false` si le bus est plein.
    pub fn publish(&self, message: AgentMessage) -> bool {
        let mut pos = self.enqueue_pos.0.load(Ordering::Relaxed);
        loop {
            let cell = &self.buffer[pos & IPC_MASK];
            let seq = cell.sequence.load(Ordering::Acquire);
            let diff = seq as isize - pos as isize;
            if diff == 0 {
                match self.enqueue_pos.0.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // SAFETY:
                        // 1. CAS on `enqueue_pos` succeeded — this thread exclusively owns cell
                        //    at index `pos & IPC_MASK` for writing. No other producer can write
                        //    to the same cell until a consumer releases it.
                        // 2. `cell.message.get()` returns a raw pointer to the inner UnsafeCell
                        //    — safe to dereference because we have exclusive ownership of this slot.
                        // 3. The write of `message` is a simple Copy type (AgentMessage is Copy);
                        //    all fields are written atomically or are pointer-sized values.
                        // 4. The Release store on `sequence` (line below) publishes this write —
                        //    a consumer's Acquire load of `sequence` will see the complete message.
                        // INVARIANT: We must NOT access `cell.message` again after releasing the
                        //    sequence token (which happens at the store below). No other thread
                        //    can read this cell until `sequence` is updated.
                        // FAILURE: If we wrote to `cell.message` without exclusive ownership, two
                        //    threads could write simultaneously — UB. The CAS prevents this.
                        unsafe {
                            *cell.message.get() = message;
                        }
                        cell.sequence.store(pos.wrapping_add(1), Ordering::Release);
                        return true;
                    }
                    Err(actual) => pos = actual,
                }
            } else if diff < 0 {
                return false; // plein
            } else {
                pos = self.enqueue_pos.0.load(Ordering::Relaxed);
            }
        }
    }

    /// Consomme le prochain message (lock-free). `None` si vide.
    pub fn dequeue(&self) -> Option<AgentMessage> {
        let mut pos = self.dequeue_pos.0.load(Ordering::Relaxed);
        loop {
            let cell = &self.buffer[pos & IPC_MASK];
            let seq = cell.sequence.load(Ordering::Acquire);
            let diff = seq as isize - (pos.wrapping_add(1)) as isize;
            if diff == 0 {
                match self.dequeue_pos.0.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // SAFETY:
                        // 1. CAS on `dequeue_pos` succeeded — this thread exclusively owns cell
                        //    at index `pos & IPC_MASK` for reading. The producer wrote this cell
                        //    at `sequence == pos` and then stored `sequence = pos + 1` with
                        //    Release ordering — our Acquire load of `sequence` (above) ensures
                        //    the complete `message` is visible.
                        // 2. `cell.message.get()` returns a valid pointer to the inner UnsafeCell.
                        //    We dereference it because we have exclusive read ownership (the
                        //    producer will not touch this cell again — it moved on to `pos+1`).
                        // 3. The copy is a bitwise copy of AgentMessage (Copy type) — no
                        //    destructor runs, no allocation happens.
                        // 4. After this read, we store `sequence = pos + IPC_QUEUE_SIZE` with
                        //    Release ordering, which "frees" the cell for the producer to reuse.
                        // INVARIANT: We must read the message BEFORE releasing the cell via the
                        //    sequence store. After the store, the producer can overwrite this cell.
                        // FAILURE: If we read after the producer overwrites (impossible due to
                        //    sequence ordering), we'd see stale/torn data. The algorithm prevents this.
                        let msg = unsafe { *cell.message.get() };
                        // Libère la case pour le tour suivant (producteur à pos+SIZE).
                        cell.sequence
                            .store(pos.wrapping_add(IPC_QUEUE_SIZE), Ordering::Release);
                        return Some(msg);
                    }
                    Err(actual) => pos = actual,
                }
            } else if diff < 0 {
                return None; // vide
            } else {
                pos = self.dequeue_pos.0.load(Ordering::Relaxed);
            }
        }
    }

    /// Réception ciblée. ATTENTION : filtrer par contenu sur un ring partagé est
    /// un anti-pattern (réordonne, n'est pas équitable). Ici c'est sans UB et sans
    /// perte (le slot libéré par le dequeue est re-rempli par republish jusqu'à
    /// succès), mais le routage PROPRE = une file MPSC par agent (chantier suivant).
    pub fn try_recv(&self, agent_id: u32) -> Option<AgentMessage> {
        match self.dequeue() {
            Some(msg) => {
                if msg.target_agent_id == agent_id || msg.target_agent_id == 0xFFFF_FFFF {
                    Some(msg)
                } else {
                    while !self.publish(msg) {
                        std::hint::spin_loop();
                    }
                    None
                }
            }
            None => None,
        }
    }

    pub const fn capacity(&self) -> usize {
        IPC_QUEUE_SIZE
    }

    /// Snapshot non atomique du nombre de messages en attente.
    pub fn pending_count(&self) -> usize {
        self.enqueue_pos
            .0
            .load(Ordering::Acquire)
            .wrapping_sub(self.dequeue_pos.0.load(Ordering::Acquire))
    }
}

impl Default for InterAgentBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_and_dequeue() {
        let bus = InterAgentBus::new();
        let payload = Box::into_raw(Box::new([1u8, 2, 3, 4]));
        let msg = AgentMessage {
            source_agent_id: 0,
            target_agent_id: 1,
            signal_code: 42,
            payload_ptr: payload as *mut u8,
            payload_size: 4,
        };
        assert!(bus.publish(msg));
        let r = bus.dequeue().expect("message attendu");
        assert_eq!(r.signal_code, 42);
        unsafe {
            drop(Box::from_raw(r.payload_ptr as *mut [u8; 4]));
        }
    }

    #[test]
    fn bus_rejects_when_full() {
        let bus = InterAgentBus::new();
        for i in 0..bus.capacity() {
            let msg = AgentMessage {
                source_agent_id: 0,
                target_agent_id: 0xFFFF_FFFF,
                signal_code: i as u32,
                payload_ptr: std::ptr::null_mut(),
                payload_size: 0,
            };
            assert!(bus.publish(msg), "publish a échoué à l'index {}", i);
        }
        let overflow = AgentMessage {
            source_agent_id: 0,
            target_agent_id: 0xFFFF_FFFF,
            signal_code: 999,
            payload_ptr: std::ptr::null_mut(),
            payload_size: 0,
        };
        assert!(!bus.publish(overflow));
    }

    #[test]
    fn mpmc_stress() {
        use std::sync::atomic::AtomicU32;
        use std::sync::Arc;
        use std::thread;

        let bus = Arc::new(InterAgentBus::new());
        let consumed = Arc::new(AtomicU32::new(0));
        let (np, nc, per) = (8usize, 4usize, 1024u32);
        let total = np as u32 * per;
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut handles = Vec::new();

        for _ in 0..np {
            let b = Arc::clone(&bus);
            handles.push(thread::spawn(move || {
                for i in 0..per {
                    let msg = AgentMessage {
                        source_agent_id: 0,
                        target_agent_id: 0xFFFF_FFFF,
                        signal_code: i,
                        payload_ptr: std::ptr::null_mut(),
                        payload_size: 0,
                    };
                    while !b.publish(msg) {
                        thread::yield_now();
                    }
                }
            }));
        }
        let mut cons = Vec::new();
        for _ in 0..nc {
            let b = Arc::clone(&bus);
            let c = Arc::clone(&consumed);
            let d = Arc::clone(&done);
            cons.push(thread::spawn(move || loop {
                match b.dequeue() {
                    Some(_) => {
                        c.fetch_add(1, Ordering::Relaxed);
                    }
                    None => {
                        if d.load(Ordering::Acquire) && b.pending_count() == 0 {
                            break;
                        }
                        thread::yield_now();
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("producteur paniqué");
        }
        done.store(true, Ordering::Release);
        for h in cons {
            h.join().expect("consommateur paniqué");
        }
        assert_eq!(consumed.load(Ordering::SeqCst), total, "messages perdus");
    }

    #[test]
    fn mpmc_no_loss_no_dup() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        use std::thread;

        let bus = Arc::new(InterAgentBus::new());
        let done = Arc::new(AtomicBool::new(false));
        let (np, per) = (8usize, 1000u32);
        let total = np as u32 * per;

        let mut prod = Vec::new();
        for p in 0..np {
            let b = Arc::clone(&bus);
            prod.push(thread::spawn(move || {
                for i in 0..per {
                    let code = p as u32 * per + i; // identifiant global unique
                    let msg = AgentMessage {
                        source_agent_id: p as u32,
                        target_agent_id: 0xFFFF_FFFF,
                        signal_code: code,
                        payload_ptr: std::ptr::null_mut(),
                        payload_size: 0,
                    };
                    while !b.publish(msg) {
                        thread::yield_now();
                    }
                }
            }));
        }
        let mut cons = Vec::new();
        for _ in 0..4 {
            let b = Arc::clone(&bus);
            let d = Arc::clone(&done);
            cons.push(thread::spawn(move || {
                let mut local: Vec<u32> = Vec::new();
                loop {
                    match b.dequeue() {
                        Some(m) => local.push(m.signal_code),
                        None => {
                            if d.load(Ordering::Acquire) && b.pending_count() == 0 {
                                break;
                            }
                            thread::yield_now();
                        }
                    }
                }
                local
            }));
        }
        for h in prod {
            h.join().expect("producteur paniqué");
        }
        done.store(true, Ordering::Release);
        let mut all: Vec<u32> = Vec::new();
        for h in cons {
            all.extend(h.join().expect("consommateur paniqué"));
        }
        all.sort_unstable();
        assert_eq!(
            all.len() as u32,
            total,
            "PERTE : {} reçus / {}",
            all.len(),
            total
        );
        let mut dd = all.clone();
        dd.dedup();
        assert_eq!(dd.len(), all.len(), "DOUBLON détecté");
        assert_eq!(all, (0..total).collect::<Vec<u32>>(), "ensemble incorrect");
    }
}
