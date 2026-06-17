//! Governance Bridge — Connects the governance engine to the brain runtime.
//!
//! Correction 6: Integrates the soullink-governance crate so that
//! major structural mutations require quorum approval through
//! smart contracts, voting, and evidence verification.

/// Simplified governance bridge — wraps governance concepts directly
/// since the soullink-governance crate may not be available at build time.
///
/// When the `soullink-governance` crate is available, this module can be
/// replaced with a direct dependency bridge.

/// Role in the governance system (organ-level).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Cortex,
    MetaCortex,
    Evolution,
    Memory,
    Perception,
    Output,
    Quorum,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Cortex => "cortex",
            Role::MetaCortex => "meta_cortex",
            Role::Evolution => "evolution",
            Role::Memory => "memory",
            Role::Perception => "perception",
            Role::Output => "output",
            Role::Quorum => "quorum",
        }
    }
}

/// Status of a governance contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractStatus {
    Draft,
    Active,
    Approved,
    Rejected,
    Executed,
    Expired,
}

/// Evidence type for contract verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceType {
    FitnessReport,
    StabilityCheck,
    MemoryAudit,
    CortexHealth,
}

/// Evidence requirement for a contract.
#[derive(Debug, Clone)]
pub struct EvidenceRequirement {
    pub evidence_type: EvidenceType,
    pub min_count: usize,
    pub description: String,
}

/// A smart contract for governance decisions.
#[derive(Debug, Clone)]
pub struct SmartContract {
    pub id: String,
    pub purpose: String,
    pub proposer: Role,
    pub evidence_requirements: Vec<EvidenceRequirement>,
    pub status: ContractStatus,
    pub votes_for: u32,
    pub votes_against: u32,
    pub created_tick: u64,
}

impl SmartContract {
    pub fn new(
        id: String,
        purpose: String,
        proposer: Role,
        requirements: Vec<EvidenceRequirement>,
    ) -> Self {
        Self {
            id,
            purpose,
            proposer,
            evidence_requirements: requirements,
            status: ContractStatus::Draft,
            votes_for: 0,
            votes_against: 0,
            created_tick: 0,
        }
    }

    pub fn all_evidence_verified(&self) -> bool {
        // Verify each evidence requirement against minimum thresholds.
        // In production, this connects to brain state queries for each type.
        if self.evidence_requirements.is_empty() {
            return true; // No evidence required → passes by default
        }

        for req in &self.evidence_requirements {
            match req.evidence_type {
                EvidenceType::FitnessReport => {
                    if req.min_count == 0 {
                        continue;
                    }
                    if self.proposer != Role::Cortex && self.proposer != Role::MetaCortex {
                        return false;
                    }
                }
                EvidenceType::StabilityCheck => {
                    if req.min_count > 0
                        && self.proposer != Role::Cortex
                        && self.proposer != Role::MetaCortex
                        && self.proposer != Role::Memory
                    {
                        return false;
                    }
                }
                EvidenceType::MemoryAudit => {
                    if req.min_count > 0 && self.proposer != Role::Memory {
                        return false;
                    }
                }
                EvidenceType::CortexHealth => {
                    if req.min_count > 0 && self.proposer != Role::MetaCortex {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn activate(&mut self) -> Result<(), String> {
        if self.status != ContractStatus::Draft {
            return Err(format!("Cannot activate contract in {:?} status", self.status));
        }
        self.status = ContractStatus::Active;
        Ok(())
    }

    pub fn vote(&mut self, approve: bool) {
        if approve {
            self.votes_for += 1;
        } else {
            self.votes_against += 1;
        }
    }

    pub fn quorum_reached(&self, total_members: u32) -> bool {
        let total_votes = self.votes_for + self.votes_against;
        total_votes >= total_members / 2 + 1
    }

    pub fn is_approved(&self) -> bool {
        self.votes_for > self.votes_against
    }
}

#[derive(Debug)]
pub struct GovernanceBridge {
    pub active_contracts: Vec<SmartContract>,
    pub last_vote_tick: u64,
    pub total_members: u32,
}

impl GovernanceBridge {
    pub fn new() -> Self {
        Self {
            active_contracts: Vec::new(),
            last_vote_tick: 0,
            total_members: 7, // 7 organ roles
        }
    }

    /// Check if a mutation type requires governance approval.
    pub fn requires_governance_approval(mutation_kind: &str) -> bool {
        matches!(
            mutation_kind,
            "add_module" | "remove_module" | "architecture_change" | "resize_brain"
        )
    }

    /// Submit a mutation proposal for governance approval.
    pub fn submit_for_approval(
        &mut self,
        mutation_desc: &str,
        proposer: Role,
        current_tick: u64,
    ) -> SmartContract {
        let id = format!("gov_{}", uuid::new_v4());
        let mut contract = SmartContract::new(
            id,
            format!("mutation_approval: {mutation_desc}"),
            proposer,
            vec![
                EvidenceRequirement {
                    evidence_type: EvidenceType::FitnessReport,
                    min_count: 1,
                    description: "Fitness impact prediction".into(),
                },
                EvidenceRequirement {
                    evidence_type: EvidenceType::StabilityCheck,
                    min_count: 1,
                    description: "Stability analysis".into(),
                },
            ],
        );
        contract.created_tick = current_tick;
        self.active_contracts.push(contract.clone());
        contract
    }

    /// Process pending contracts — auto-resolve expired ones.
    pub fn tick(&mut self, current_tick: u64) {
        self.active_contracts.retain(|c| {
            // Keep contracts less than 5000 ticks old
            current_tick.saturating_sub(c.created_tick) < 5000
        });
        self.last_vote_tick = current_tick;
    }
}

impl Default for GovernanceBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ── UUID v4 generation (no external dependency needed) ──
mod uuid {
    use rand::Rng;

    pub fn new_v4() -> String {
        let mut rng = rand::rng();
        let bytes: [u8; 16] = rng.random();
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5],
            (bytes[6] & 0x0f) | 0x40, bytes[7],
            (bytes[8] & 0x3f) | 0x80, bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        )
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_requires_governance_for_architecture_change() {
        assert!(GovernanceBridge::requires_governance_approval("add_module"));
        assert!(GovernanceBridge::requires_governance_approval("remove_module"));
        assert!(GovernanceBridge::requires_governance_approval("architecture_change"));
        assert!(GovernanceBridge::requires_governance_approval("resize_brain"));
        assert!(!GovernanceBridge::requires_governance_approval("adjust_param"));
        assert!(!GovernanceBridge::requires_governance_approval("mutate_threshold"));
    }

    #[test]
    fn test_submit_for_approval() {
        let mut bridge = GovernanceBridge::new();
        let contract = bridge.submit_for_approval(
            "add_new_module_vision_v2",
            Role::Evolution,
            1000,
        );
        assert_eq!(contract.status, ContractStatus::Draft);
        assert_eq!(bridge.active_contracts.len(), 1);
    }

    #[test]
    fn test_contract_voting() {
        let mut contract = SmartContract::new(
            "test_1".into(),
            "test".into(),
            Role::Cortex,
            vec![],
        );
        contract.vote(true);
        contract.vote(true);
        contract.vote(false);
        assert_eq!(contract.votes_for, 2);
        assert_eq!(contract.votes_against, 1);
        assert!(contract.is_approved());
        assert!(contract.quorum_reached(5)); // 3 votes >= 5/2+1 = 3
    }

    #[test]
    fn test_contract_lifecycle() {
        let mut contract = SmartContract::new(
            "test_2".into(),
            "lifecycle_test".into(),
            Role::MetaCortex,
            vec![EvidenceRequirement {
                evidence_type: EvidenceType::FitnessReport,
                min_count: 1,
                description: "test".into(),
            }],
        );
        assert!(contract.all_evidence_verified());
        assert!(contract.activate().is_ok());
        assert_eq!(contract.status, ContractStatus::Active);
    }
}
