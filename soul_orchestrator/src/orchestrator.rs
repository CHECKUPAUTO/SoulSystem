//! Couche au-dessus de `soul_ipc::InterAgentBus`. Chaque agent possede SA propre
//! mailbox (un `InterAgentBus` borne MPMC reutilise en MPSC) : c'est le routage
//! "file par agent" que `soul_ipc::try_recv` designe comme la bonne approche,
//! par opposition au filtrage par contenu sur un ring partage (qui reordonne et
//! n'est pas equitable).
//!
//! Concurrence : `register_agent` au setup (`&mut self`) ; le chemin chaud
//! (`dispatch`/`poll`/`wake`/`transition`) est `&self` sans verrou (table de
//! routage en lecture seule, coordination par atomiques). Enregistrement
//! dynamique en cours de run = envelopper la table dans RwLock/arc-swap (hors v1).
//!
//! Propriete du payload (`AgentMessage.payload_ptr` brut) : dispatch cible ->
//! propriete transferee a l'unique consommateur (`poll`) ; broadcast -> payload
//! DOIT etre nul ou pointer une memoire que l'appelant maintient et libere
//! lui-meme (les consommateurs d'un broadcast ne liberent jamais : sinon
//! double-free). Le broadcast vise des signaux, pas un transfert de propriete.

use soul_ipc::{AgentMessage, InterAgentBus};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use prometheus::{IntCounter, IntGauge, Opts, Registry};
use lazy_static::lazy_static;

lazy_static! {
    static ref ORCHESTRATOR_REGISTRY: Registry = Registry::new();
    
    static ref DISPATCH_TOTAL: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_orchestrator_dispatch_total",
        "Total number of message dispatches"
    )).unwrap();
    
    static ref DISPATCH_DELIVERED: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_orchestrator_dispatch_delivered_total",
        "Total delivered messages"
    )).unwrap();
    
    static ref DISPATCH_BROADCAST: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_orchestrator_dispatch_broadcast_total",
        "Total broadcast messages"
    )).unwrap();
    
    static ref AGENTS_REGISTERED: IntGauge = IntGauge::with_opts(Opts::new(
        "soul_orchestrator_agents_registered",
        "Number of registered agents"
    )).unwrap();
    
    static ref AGENTS_ACTIVE: IntGauge = IntGauge::with_opts(Opts::new(
        "soul_orchestrator_agents_active",
        "Number of active agents"
    )).unwrap();
    
    static ref MAILBOX_FULL: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_orchestrator_mailbox_full_total",
        "Total mailbox full events (back-pressure)"
    )).unwrap();
    
    static ref WAKE_TOTAL: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_orchestrator_wake_total",
        "Total agent wake events"
    )).unwrap();
    
    static ref TRANSITION_TOTAL: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_orchestrator_transition_total",
        "Total state transitions"
    )).unwrap();
}

fn register_metrics() {
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(DISPATCH_TOTAL.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(DISPATCH_DELIVERED.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(DISPATCH_BROADCAST.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(AGENTS_REGISTERED.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(AGENTS_ACTIVE.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(MAILBOX_FULL.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(WAKE_TOTAL.clone()));
    let _ = ORCHESTRATOR_REGISTRY.register(Box::new(TRANSITION_TOTAL.clone()));
}

/// Sentinelle de diffusion partagee avec `soul_ipc` : message destine a tous.
pub const BROADCAST: u32 = 0xFFFF_FFFF;

/// Etat d'eveil d'un agent, encode sur un octet pour un `AtomicU8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AgentState {
    Dormant = 0,
    Active = 1,
    HyperFocus = 2,
}

impl AgentState {
    #[inline]
    fn from_u8(v: u8) -> Self {
        match v {
            1 => AgentState::Active,
            2 => AgentState::HyperFocus,
            _ => AgentState::Dormant,
        }
    }

    #[inline]
    fn may_transition_to(self, next: AgentState) -> bool {
        use AgentState::*;
        matches!(
            (self, next),
            (Dormant, Active) | (Active, Dormant) | (Active, HyperFocus) | (HyperFocus, Active)
        )
    }
}

/// Resultat d'un `dispatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    Delivered { woke: bool },
    Broadcast {
        delivered: usize,
        full: usize,
        woke: usize,
    },
    UnknownTarget,
    MailboxFull,
}

/// Erreur de transition de cycle de vie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorError {
    UnknownAgent,
    NoOpTransition,
    IllegalTransition {
        from: AgentState,
        to: AgentState,
    },
}

/// Poignee interne : etat atomique + mailbox dediee.
struct AgentHandle {
    state: AtomicU8,
    mailbox: InterAgentBus,
}

/// Orchestrateur souverain : routage par identite + cycle de vie.
pub struct SovereignOrchestrator {
    registry: HashMap<u32, AgentHandle>,
}

impl SovereignOrchestrator {
    pub fn new() -> Self {
        register_metrics();
        Self {
            registry: HashMap::new(),
        }
    }

    pub fn register_agent(&mut self, id: u32) -> bool {
        if id == BROADCAST || self.registry.contains_key(&id) {
            return false;
        }
        self.registry.insert(
            id,
            AgentHandle {
                state: AtomicU8::new(AgentState::Dormant as u8),
                mailbox: InterAgentBus::new(),
            },
        );
        AGENTS_REGISTERED.inc();
        true
    }

    #[inline]
    pub fn is_registered(&self, id: u32) -> bool {
        self.registry.contains_key(&id)
    }

    #[inline]
    pub fn agent_count(&self) -> usize {
        self.registry.len()
    }

    pub fn state(&self, id: u32) -> Option<AgentState> {
        self.registry
            .get(&id)
            .map(|h| AgentState::from_u8(h.state.load(Ordering::Acquire)))
    }

    pub fn is_schedulable(&self, id: u32) -> bool {
        matches!(
            self.state(id),
            Some(AgentState::Active) | Some(AgentState::HyperFocus)
        )
    }

    pub fn dispatch(&self, msg: AgentMessage) -> DispatchOutcome {
        DISPATCH_TOTAL.inc();
        
        if msg.target_agent_id == BROADCAST {
            return self.broadcast(msg);
        }
        let Some(h) = self.registry.get(&msg.target_agent_id) else {
            return DispatchOutcome::UnknownTarget;
        };
        if !h.mailbox.publish(msg) {
            MAILBOX_FULL.inc();
            return DispatchOutcome::MailboxFull;
        }
        let woke = Self::wake_handle(h);
        if woke {
            WAKE_TOTAL.inc();
        }
        DISPATCH_DELIVERED.inc();
        DispatchOutcome::Delivered { woke }
    }

    fn broadcast(&self, msg: AgentMessage) -> DispatchOutcome {
        let (mut delivered, mut full, mut woke) = (0usize, 0usize, 0usize);
        for h in self.registry.values() {
            if h.mailbox.publish(msg) {
                delivered += 1;
                if Self::wake_handle(h) {
                    woke += 1;
                    WAKE_TOTAL.inc();
                }
            } else {
                full += 1;
                MAILBOX_FULL.inc();
            }
        }
        DISPATCH_BROADCAST.inc();
        DispatchOutcome::Broadcast {
            delivered,
            full,
            woke,
        }
    }

    pub fn poll(&self, id: u32) -> Option<AgentMessage> {
        self.registry.get(&id).and_then(|h| h.mailbox.dequeue())
    }

    pub fn wake(&self, id: u32) -> bool {
        let result = self.registry.get(&id).is_some_and(Self::wake_handle);
        if result {
            WAKE_TOTAL.inc();
        }
        result
    }

    #[inline]
    fn wake_handle(h: &AgentHandle) -> bool {
        h.state
            .compare_exchange(
                AgentState::Dormant as u8,
                AgentState::Active as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn transition(&self, id: u32, to: AgentState) -> Result<(), OrchestratorError> {
        let Some(h) = self.registry.get(&id) else {
            return Err(OrchestratorError::UnknownAgent);
        };
        loop {
            let cur_u8 = h.state.load(Ordering::Acquire);
            let cur = AgentState::from_u8(cur_u8);
            if cur == to {
                return Err(OrchestratorError::NoOpTransition);
            }
            if !cur.may_transition_to(to) {
                return Err(OrchestratorError::IllegalTransition { from: cur, to });
            }
            match h.state.compare_exchange_weak(
                cur_u8,
                to as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    TRANSITION_TOTAL.inc();
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }
    
    pub fn update_active_agents_metric(&self) {
        let active = self.registry.values()
            .filter(|h| {
                let state = AgentState::from_u8(h.state.load(Ordering::Acquire));
                matches!(state, AgentState::Active | AgentState::HyperFocus)
            })
            .count();
        AGENTS_ACTIVE.set(active as i64);
    }
}

impl Default for SovereignOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn signal(target: u32, code: u32) -> AgentMessage {
        AgentMessage {
            source_agent_id: 0,
            target_agent_id: target,
            signal_code: code,
            payload_ptr: std::ptr::null_mut(),
            payload_size: 0,
        }
    }

    #[test]
    fn register_and_count() {
        let mut o = SovereignOrchestrator::new();
        assert!(o.register_agent(101));
        assert!(o.register_agent(102));
        assert_eq!(o.agent_count(), 2);
        assert!(!o.register_agent(101), "doublon refuse");
        assert!(!o.register_agent(BROADCAST), "sentinelle interdite");
        assert_eq!(o.agent_count(), 2);
        assert!(o.is_registered(101));
        assert!(!o.is_registered(999));
    }

    #[test]
    fn dispatch_routes_to_target_mailbox() {
        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        o.register_agent(2);
        assert_eq!(
            o.dispatch(signal(1, 42)),
            DispatchOutcome::Delivered { woke: true }
        );
        assert_eq!(o.poll(1).expect("message pour 1").signal_code, 42);
        assert!(o.poll(2).is_none(), "2 ne recoit rien");
        assert!(o.poll(1).is_none(), "mailbox de 1 videe");
    }

    #[test]
    fn dispatch_unknown_target() {
        let o = SovereignOrchestrator::new();
        assert_eq!(o.dispatch(signal(7, 1)), DispatchOutcome::UnknownTarget);
    }

    #[test]
    fn dispatch_wakes_dormant_once() {
        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        assert_eq!(o.state(1), Some(AgentState::Dormant));
        assert_eq!(
            o.dispatch(signal(1, 1)),
            DispatchOutcome::Delivered { woke: true }
        );
        assert_eq!(o.state(1), Some(AgentState::Active));
        assert_eq!(
            o.dispatch(signal(1, 2)),
            DispatchOutcome::Delivered { woke: false }
        );
    }

    #[test]
    fn mailbox_full_reported() {
        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        let cap = InterAgentBus::new().capacity();
        for i in 0..cap {
            assert!(
                matches!(
                    o.dispatch(signal(1, i as u32)),
                    DispatchOutcome::Delivered { .. }
                ),
                "dispatch {i} devrait reussir"
            );
        }
        assert_eq!(o.dispatch(signal(1, 9999)), DispatchOutcome::MailboxFull);
    }

    #[test]
    fn broadcast_fans_out_and_wakes_all() {
        let mut o = SovereignOrchestrator::new();
        for id in [10u32, 20, 30] {
            o.register_agent(id);
        }
        assert_eq!(
            o.dispatch(signal(BROADCAST, 7)),
            DispatchOutcome::Broadcast {
                delivered: 3,
                full: 0,
                woke: 3
            }
        );
        for id in [10u32, 20, 30] {
            assert_eq!(o.poll(id).expect("copie broadcast").signal_code, 7);
            assert_eq!(o.state(id), Some(AgentState::Active));
        }
    }

    #[test]
    fn transition_respects_state_machine() {
        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        assert!(o.transition(1, AgentState::Active).is_ok());
        assert!(o.transition(1, AgentState::HyperFocus).is_ok());
        assert_eq!(o.state(1), Some(AgentState::HyperFocus));
        assert_eq!(
            o.transition(1, AgentState::Dormant),
            Err(OrchestratorError::IllegalTransition {
                from: AgentState::HyperFocus,
                to: AgentState::Dormant
            })
        );
        assert!(o.transition(1, AgentState::Active).is_ok());
        assert_eq!(
            o.transition(1, AgentState::Active),
            Err(OrchestratorError::NoOpTransition)
        );
        assert!(o.transition(1, AgentState::Dormant).is_ok());
        assert_eq!(
            o.transition(42, AgentState::Active),
            Err(OrchestratorError::UnknownAgent)
        );
    }

    #[test]
    fn is_schedulable_tracks_state() {
        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        assert!(!o.is_schedulable(1));
        o.wake(1);
        assert!(o.is_schedulable(1));
        o.transition(1, AgentState::HyperFocus)
            .expect("Active->HyperFocus");
        assert!(o.is_schedulable(1));
        assert!(!o.is_schedulable(999));
    }

    #[test]
    fn concurrent_producers_single_consumer_no_loss() {
        use std::sync::Arc;
        use std::thread;

        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        let o = Arc::new(o);

        let (n_prod, per) = (8usize, 2000u32);
        let total = n_prod as u64 * per as u64;

        let done = Arc::new(AtomicBool::new(false));
        let mut producers = Vec::new();
        for _ in 0..n_prod {
            let oc = Arc::clone(&o);
            producers.push(thread::spawn(move || {
                for i in 0..per {
                    loop {
                        match oc.dispatch(signal(1, i)) {
                            DispatchOutcome::Delivered { .. } => break,
                            DispatchOutcome::MailboxFull => std::hint::spin_loop(),
                            other => panic!("dispatch inattendu: {other:?}"),
                        }
                    }
                }
            }));
        }

        let oc = Arc::clone(&o);
        let dc = Arc::clone(&done);
        let consumer = thread::spawn(move || {
            let mut got = 0u64;
            while got < total {
                if oc.poll(1).is_some() {
                    got += 1;
                } else if dc.load(Ordering::Acquire) {
                    while oc.poll(1).is_some() {
                        got += 1;
                    }
                    break;
                } else {
                    std::hint::spin_loop();
                }
            }
            got
        });

        for p in producers {
            p.join().expect("producteur");
        }
        done.store(true, Ordering::Release);
        assert_eq!(
            consumer.join().expect("consommateur"),
            total,
            "aucun message perdu"
        );
    }
}