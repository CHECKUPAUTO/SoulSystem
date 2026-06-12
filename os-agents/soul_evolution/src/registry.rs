/// Agent registry bridge.
///
/// Maps `.agents/agents/*.md` files (YAML frontmatter + markdown body) to `u32` agent IDs
/// understood by the `SovereignOrchestrator`. This is the central coupling point between
/// the filesystem agent ecosystem and the runtime orchestrator.
///
/// Agent IDs are computed as an FNV-1a hash of the lowercase agent name, ensuring
/// deterministic and collision-resistant mapping across restarts.

use crate::audit::Auditor;
use crate::types::EvolutionConfig;
use soul_orchestrator::SovereignOrchestrator;
use std::collections::HashMap;
use std::path::Path;

/// A lightweight handle produced by scanning an agent .md file on disk.
#[derive(Debug, Clone)]
pub struct AgentRegistryEntry {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub model: String,
    pub path: String,
}

/// Scans `agents_dir` for `.md` files, parses frontmatter, and returns a
/// `HashMap<u32, AgentRegistryEntry>` ready for registration into the orchestrator.
///
/// Agent IDs are FNV-like hashes of the uppercased name, seeded with 0x811C9DC5.
/// Two different names **can** collide (standard 32-bit hash), but the probability
/// across O(100) agents is negligible.
pub fn scan_agents(agents_dir: &Path) -> HashMap<u32, AgentRegistryEntry> {
    let mut config = EvolutionConfig::default();
    config.agents_dir = agents_dir.to_path_buf();

    let audits = Auditor::audit_agents(&config);
    let mut registry = HashMap::new();

    for audit in audits {
        let name = audit.agent.name.clone();
        let id = agent_name_to_id(&name);

        let entry = AgentRegistryEntry {
            id,
            name: name.clone(),
            description: audit.agent.description.clone(),
            model: audit.agent.model.clone(),
            path: audit.agent.path.to_string_lossy().to_string(),
        };

        registry.insert(id, entry);
    }

    registry
}

/// Registers every entry from `scan_agents` into the orchestrator.
///
/// Returns the number of agents successfully registered.  Entries that already
/// exist in the orchestrator are skipped (idempotent).
pub fn register_agents(
    orchestrator: &mut SovereignOrchestrator,
    entries: &HashMap<u32, AgentRegistryEntry>,
) -> usize {
    let mut registered = 0;
    for (&id, entry) in entries {
        if orchestrator.register_agent(id) {
            registered += 1;
            tracing::info!(
                agent_id = id,
                agent_name = %entry.name,
                "Registered agent from disk"
            );
        }
    }
    registered
}

/// Convenience: scan + register in one call.
///
/// Returns the agent entries that were registered (both new and already-existing).
pub fn load_and_register(
    orchestrator: &mut SovereignOrchestrator,
    agents_dir: &Path,
) -> HashMap<u32, AgentRegistryEntry> {
    let entries = scan_agents(agents_dir);
    let count = register_agents(orchestrator, &entries);
    tracing::info!(
        scanned = entries.len(),
        newly_registered = count,
        "Agent registry bootstrapped from disk"
    );
    entries
}

/// Deterministic FNV-1a hash that maps an agent name to a `u32` orchestrator ID.
///
/// The name is used in its lowercase form so that `AgentEvolver` and
/// `agent-evolver` produce the same ID.  Special-case: the broadcast ID
/// (`0xFFFF_FFFF`) is never returned.
pub fn agent_name_to_id(name: &str) -> u32 {
    const FNV_OFFSET: u32 = 0x811C9DC5;
    const FNV_PRIME: u32 = 0x01000193;

    let bytes = name.to_lowercase().into_bytes();
    let mut hash: u32 = FNV_OFFSET;

    for byte in bytes {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Never return the BROADCAST sentinel.
    if hash == 0xFFFF_FFFF {
        hash = 0xFFFF_FFFE;
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_name_to_id_deterministic() {
        let a = agent_name_to_id("agent-evolver");
        let b = agent_name_to_id("agent-evolver");
        assert_eq!(a, b);
    }

    #[test]
    fn test_agent_name_to_id_case_insensitive() {
        let a = agent_name_to_id("AGENT-EVOLVER");
        let b = agent_name_to_id("agent-evolver");
        assert_eq!(a, b);
    }

    #[test]
    fn test_agent_name_to_id_not_broadcast() {
        // Ensure no agent name produces the broadcast ID.
        let names = ["agent-a", "agent-b", "agent-evolver", "zzz-top"];
        for name in names {
            assert_ne!(agent_name_to_id(name), 0xFFFF_FFFF);
        }
    }

    #[test]
    fn test_agent_name_to_id_different_names() {
        let a = agent_name_to_id("agent-evolver");
        let b = agent_name_to_id("agent-auditor");
        assert_ne!(a, b);
    }
}
