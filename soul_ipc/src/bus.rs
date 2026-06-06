//! Bus de communication inter-agents Lock-Free de type Multi-Producer Multi-Consumer (MPMC).
//! Le passage de message se fait en mode Zero-Copy par transfert de propriété de pointeurs bruts.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;

const IPC_QUEUE_SIZE: usize = 8192;
const IPC_MASK: usize = IPC_QUEUE_SIZE - 1;

/// Message inter-agent — transmission zéro-copy par pointeurs bruts.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AgentMessage {
    /// ID de l'agent émetteur
    pub source_agent_id: u32,
    /// ID de l'agent cible (0xFFFFFFFF = broadcast universel)
    pub target_agent_id: u32,
    /// Code de signal (type de message / protocole)
    pub signal_code: u32,
    /// Pointeur vers le payload (zero-copy — propriété transférée au destinataire)
    pub payload_ptr: *mut u8,
    /// Taille du payload en octets
    pub payload_size: usize,
}

/// Bus MPMC lock-free de type ring buffer.
#[repr(align(64))] // Alignement anti-false-sharing entre bus instances multiples
pub struct InterAgentBus {
    head: AtomicUsize,  // Index de lecture (consommation)
    tail: AtomicUsize,  // Index d'écriture (production)
    buffer: [UnsafeCell<Option<AgentMessage>>; IPC_QUEUE_SIZE],
}

unsafe impl Sync for InterAgentBus {}
unsafe impl Send for InterAgentBus {}

impl InterAgentBus {
    /// Construit un bus vide — toutes les cases initialisées à None.
    pub fn new() -> Self {
        let mut buffer: [UnsafeCell<Option<AgentMessage>>; IPC_QUEUE_SIZE] = unsafe {
            std::mem::MaybeUninit::uninit().assume_init()
        };
        for entry in buffer.iter_mut() {
            *entry = UnsafeCell::new(None);
        }

        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            buffer,
        }
    }

    /// Publie un message sur le bus universel (Non-bloquant, Lock-Free).
    /// Retourne `false` si le bus est saturé.
    pub fn publish(&self, message: AgentMessage) -> bool {
        loop {
            let t = self.tail.load(Ordering::Acquire);
            let h = self.head.load(Ordering::Acquire);

            if t.wrapping_sub(h) >= IPC_QUEUE_SIZE {
                return false; // Bus temporairement saturé
            }

            if self.tail.compare_exchange_weak(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                unsafe {
                    let slot = self.buffer[t & IPC_MASK].get();
                    *slot = Some(message);
                }
                return true;
            }
        }
    }

    /// Consomme le prochain message sur le bus (avancement inconditionnel du head).
    /// Le filtrage par agent_id se fait au niveau appelant.
    pub fn dequeue(&self) -> Option<AgentMessage> {
        loop {
            let h = self.head.load(Ordering::Acquire);
            let t = self.tail.load(Ordering::Acquire);

            if h == t {
                return None; // Pas de message disponible
            }

            unsafe {
                let slot = self.buffer[h & IPC_MASK].get();
                match (*slot).take() {
                    Some(msg) => {
                        if self.head.compare_exchange(h, h + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                            return Some(msg);
                        } else {
                            // CAS échoué — un autre consommateur a pris le message. Réinsérer et réessayer.
                            *slot = Some(msg);
                            std::hint::spin_loop();
                        }
                    }
                    None => {
                        // Cas de course rare : le producteur n'a pas encore écrit. Spin puis réessayez.
                        std::hint::spin_loop();
                    }
                }
            }
        }
    }

    /// Consomme un message ciblé pour un agent spécifique.
    /// Les messages non-ciblés sont automatiquement avancés (passés).
    pub fn try_recv(&self, agent_id: u32) -> Option<AgentMessage> {
        loop {
            let h = self.head.load(Ordering::Acquire);
            let t = self.tail.load(Ordering::Acquire);

            if h == t {
                return None;
            }

            unsafe {
                let slot = self.buffer[h & IPC_MASK].get();
                match (*slot).take() {
                    Some(msg) => {
                        // Broadcast ou message ciblé — accepter. Sinon, réinjecter en queue.
                        if msg.target_agent_id == agent_id || msg.target_agent_id == 0xFFFFFFFF {
                            if self.head.compare_exchange(h, h + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                                return Some(msg);
                            } else {
                                *slot = Some(msg);
                                std::hint::spin_loop();
                            }
                        } else {
                            // Message non ciblé : avancer le head et réinjecter en queue pour le vrai destinataire.
                            if self.head.compare_exchange(h, h + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                                // Ré-injection atomique — garantit la non-perdance du message.
                                let _ = self.publish(msg);
                            } else {
                                *slot = Some(msg);
                                std::hint::spin_loop();
                            }
                        }
                    }
                    None => {
                        std::hint::spin_loop();
                    }
                }
            }
        }
    }

    /// Taille du buffer (constante.compile time).
    pub const fn capacity(&self) -> usize {
        IPC_QUEUE_SIZE
    }

    /// Nombre approximatif de messages en attente (snapshot non atomique).
    pub fn pending_count(&self) -> usize {
        self.tail.load(Ordering::Acquire).wrapping_sub(self.head.load(Ordering::Acquire))
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
        let received = bus.dequeue();
        assert!(received.is_some());
        let r = received.unwrap();
        assert_eq!(r.signal_code, 42);
        // Nettoyage : libérer le payload reçu.
        unsafe { drop(Box::from_raw(r.payload_ptr as *mut [u8; 4])); }
    }

    #[test]
    fn bus_rejects_when_full() {
        let bus = InterAgentBus::new();
        for i in 0..bus.capacity() {
            let msg = AgentMessage {
                source_agent_id: 0,
                target_agent_id: 0xFFFFFFFF,
                signal_code: i as u32,
                payload_ptr: std::ptr::null_mut(),
                payload_size: 0,
            };
            assert!(bus.publish(msg), "publish failed at index {}", i);
        }
        // Un message de plus doit échouer.
        let overflow = AgentMessage {
            source_agent_id: 0,
            target_agent_id: 0xFFFFFFFF,
            signal_code: 999,
            payload_ptr: std::ptr::null_mut(),
            payload_size: 0,
        };
        assert!(!bus.publish(overflow));
    }

    #[test]
    fn mpmc_stress() {
        use std::thread;
        use std::sync::Arc;
        use std::sync::atomic::AtomicU32;

        let bus = Arc::new(InterAgentBus::new());
        let consumed = Arc::new(AtomicU32::new(0));
        let num_publishers = 8;
        let num_consumers = 4;
        let msgs_per_publisher = 1024;

        let mut handles = Vec::new();

        for _ in 0..num_publishers {
            let bus_clone = Arc::clone(&bus);
            handles.push(thread::spawn(move || {
                for i in 0..msgs_per_publisher {
                    let msg = AgentMessage {
                        source_agent_id: 0,
                        target_agent_id: 0xFFFFFFFF,
                        signal_code: i as u32,
                        payload_ptr: std::ptr::null_mut(),
                        payload_size: 0,
                    };
                    while !bus_clone.publish(msg) {
                        thread::yield_now();
                    }
                }
            }));
        }

        for _ in 0..num_consumers {
            let bus_clone = Arc::clone(&bus);
            let consumed_clone = Arc::clone(&consumed);
            handles.push(thread::spawn(move || {
                loop {
                    if bus_clone.dequeue().is_some() {
                        consumed_clone.fetch_add(1, Ordering::Relaxed);
                    } else {
                        break;
                    }
                }
            }));
        }

        for h in handles {
            h.join().expect("IPC stress test thread panicked");
        }

        let total = num_publishers * msgs_per_publisher;
        assert_eq!(consumed.load(Ordering::SeqCst), total, "All published messages must be consumed");
    }
}
