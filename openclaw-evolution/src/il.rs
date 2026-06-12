use serde::{Deserialize, Serialize};
use std::fmt;

// ─────────────────────────────────────────────
// Instruction IL
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OpCode {
    /// Initialiser un sous-système
    Init,
    /// Lancer une recherche web
    Search,
    /// Filtrer les résultats
    Filter,
    /// Classer les résultats
    Rank,
    /// Stocker en mémoire
    Store,
    /// Évaluer la fitness
    Eval,
    /// Charger un agent
    Load,
    /// Muter un agent
    Mutate,
    /// Valider via sandbox
    Validate,
    /// Committer si amélioré
    Commit,
    /// Rollback si dégradé
    Rollback,
    /// Observer l'état système
    Observe,
    /// Identifier un bottleneck
    Identify,
    /// Proposer une optimisation
    Propose,
    /// Simuler dans le sandbox
    Simulate,
    /// Appliquer si sûr
    Apply,
    /// Générer des embeddings
    Embed,
    /// Basculer mode inférence
    SwitchMode,
    /// Ne rien faire
    Nop,
    /// Arrêter
    Halt,
}

impl fmt::Display for OpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ILInstruction {
    pub opcode: OpCode,
    pub operands: Vec<String>,
}

impl ILInstruction {
    pub fn new(opcode: OpCode, operands: Vec<&str>) -> Self {
        Self {
            opcode,
            operands: operands.into_iter().map(String::from).collect(),
        }
    }
}

impl fmt::Display for ILInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.opcode, self.operands.join(", "))
    }
}

// ─────────────────────────────────────────────
// Programme IL
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ILProgram {
    pub instructions: Vec<ILInstruction>,
    pub version: u64,
    pub label: String,
}

impl ILProgram {
    pub fn new(label: &str) -> Self {
        Self {
            instructions: Vec::new(),
            version: 0,
            label: label.to_string(),
        }
    }

    pub fn push(&mut self, instruction: ILInstruction) {
        self.instructions.push(instruction);
    }

    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    pub fn bump_version(&mut self) {
        self.version += 1;
    }

    /// Programme d'exploration standard
    pub fn exploration(target: &str) -> Self {
        let mut p = Self::new("exploration");
        p.push(ILInstruction::new(OpCode::Init, vec!["exploration"]));
        p.push(ILInstruction::new(OpCode::Search, vec![target]));
        p.push(ILInstruction::new(OpCode::Embed, vec!["results"]));
        p.push(ILInstruction::new(OpCode::Rank, vec!["semantic"]));
        p.push(ILInstruction::new(OpCode::Filter, vec!["top", "10"]));
        p.push(ILInstruction::new(OpCode::Store, vec!["memory"]));
        p.push(ILInstruction::new(OpCode::Eval, vec!["fitness"]));
        p
    }

    /// Programme de mutation d'agent
    pub fn mutation(agent_id: &str, mutation_type: &str) -> Self {
        let mut p = Self::new("mutation");
        p.push(ILInstruction::new(OpCode::Load, vec![agent_id]));
        p.push(ILInstruction::new(OpCode::Mutate, vec![mutation_type]));
        p.push(ILInstruction::new(OpCode::Simulate, vec!["sandbox"]));
        p.push(ILInstruction::new(
            OpCode::Validate,
            vec!["fitness_improved"],
        ));
        p.push(ILInstruction::new(OpCode::Commit, vec!["if_improved"]));
        p.push(ILInstruction::new(OpCode::Rollback, vec!["if_degraded"]));
        p
    }

    /// Programme de stagnation (bascule en mode DEEP)
    pub fn stagnation_response() -> Self {
        let mut p = Self::new("stagnation_response");
        p.push(ILInstruction::new(OpCode::Observe, vec!["fitness_history"]));
        p.push(ILInstruction::new(OpCode::Identify, vec!["stagnation"]));
        p.push(ILInstruction::new(OpCode::SwitchMode, vec!["deep"]));
        p.push(ILInstruction::new(OpCode::Mutate, vec!["structural"]));
        p.push(ILInstruction::new(OpCode::Simulate, vec!["sandbox"]));
        p.push(ILInstruction::new(OpCode::Validate, vec!["breakthrough"]));
        p.push(ILInstruction::new(OpCode::SwitchMode, vec!["fast"]));
        p
    }
}

// ─────────────────────────────────────────────
// Résultat d'exécution IL
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ILExecutionResult {
    pub program_label: String,
    pub instructions_executed: usize,
    pub instructions_total: usize,
    /// Résultats par instruction
    pub step_results: Vec<StepResult>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub opcode: String,
    pub status: StepStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Ok,
    Skipped,
    Failed,
}

impl ILExecutionResult {
    pub fn new(label: &str, total: usize) -> Self {
        Self {
            program_label: label.to_string(),
            instructions_executed: 0,
            instructions_total: total,
            step_results: Vec::new(),
            success: true,
        }
    }

    pub fn add_step(&mut self, opcode: &OpCode, status: StepStatus, detail: &str) {
        self.step_results.push(StepResult {
            opcode: format!("{}", opcode),
            status: status.clone(),
            detail: detail.to_string(),
        });
        if status != StepStatus::Skipped {
            self.instructions_executed += 1;
        }
        if status == StepStatus::Failed {
            self.success = false;
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "IL[{}] v{}: {}/{} exécutées, ok={}",
            self.program_label,
            0, // version géré par le caller
            self.instructions_executed,
            self.instructions_total,
            self.success
        )
    }
}
