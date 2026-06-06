use parking_lot::{Mutex, RwLock};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct SynapseRoute {
    pub source_id: u64,
    pub target_id: u64,
    pub weight: f32,
    pub is_active: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RoutingTable {
    pub routes: [SynapseRoute; 1024],
    pub active_count: usize,
}

pub struct SynapticLinkerAgent {
    current_table: RwLock<RoutingTable>,
    write_lock: Mutex<()>,
}

impl SynapticLinkerAgent {
    pub fn new() -> Self {
        let table = RoutingTable { routes: [SynapseRoute { source_id: 0, target_id: 0, weight: 0.0, is_active: false }; 1024], active_count: 0 };
        Self { current_table: RwLock::new(table), write_lock: Mutex::new(()) }
    }

    pub fn update_synapse_route(&self, source: u64, target: u64, weight: f32, is_active: bool) {
        let _guard = self.write_lock.lock();
        let mut new_table = *self.current_table.read();

        let mut found = false;
        for i in 0..new_table.active_count {
            if new_table.routes[i].source_id == source && new_table.routes[i].target_id == target {
                new_table.routes[i] = SynapseRoute { source_id: source, target_id: target, weight, is_active };
                found = true; break;
            }
        }
        if !found && new_table.active_count < 1024 {
            new_table.routes[new_table.active_count] = SynapseRoute { source_id: source, target_id: target, weight, is_active };
            new_table.active_count += 1;
        }
        *self.current_table.write() = new_table;
    }

    pub fn resolve_routing_weight(&self, source: u64, target: u64) -> Option<f32> {
        let table = self.current_table.read();
        for i in 0..table.active_count {
            let route = &table.routes[i];
            if route.is_active && route.source_id == source && route.target_id == target { return Some(route.weight); }
        }
        None
    }

    pub fn get_total_synapse_count(&self) -> usize { self.current_table.read().active_count }
}
