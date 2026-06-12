use crate::il::{ILInstruction, ILProgram, OpCode};
use rand::Rng;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Méta-langage
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaLanguage {
    pub grammar_rules: Vec<GrammarRule>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrammarRule {
    pub pattern: String,
    pub expansion: Vec<String>,
    pub weight: f32,
    /// Nombre de fois que cette règle a été utilisée
    pub usage_count: u64,
    /// Score de succès moyen quand cette règle est utilisée
    pub avg_success: f32,
}

impl MetaLanguage {
    pub fn new() -> Self {
        Self {
            grammar_rules: vec![
                GrammarRule {
                    pattern: "EXPLORE".into(),
                    expansion: vec!["Search".into(), "Embed".into(), "Rank".into(), "Store".into()],
                    weight: 1.0,
                    usage_count: 0,
                    avg_success: 0.5,
                },
                GrammarRule {
                    pattern: "EVOLVE".into(),
                    expansion: vec!["Load".into(), "Mutate".into(), "Simulate".into(), "Validate".into(), "Commit".into()],
                    weight: 1.0,
                    usage_count: 0,
                    avg_success: 0.5,
                },
                GrammarRule {
                    pattern: "OPTIMIZE".into(),
                    expansion: vec!["Observe".into(), "Identify".into(), "Mutate".into(), "Simulate".into(), "Apply".into()],
                    weight: 1.0,
                    usage_count: 0,
                    avg_success: 0.5,
                },
                GrammarRule {
                    pattern: "SAFE_EVOLVE".into(),
                    expansion: vec!["Load".into(), "Mutate".into(), "Simulate".into(), "Validate".into(), "Commit".into(), "Rollback".into()],
                    weight: 0.8,
                    usage_count: 0,
                    avg_success: 0.5,
                },
                GrammarRule {
                    pattern: "DEEP_EVOLVE".into(),
                    expansion: vec!["Observe".into(), "SwitchMode".into(), "Mutate".into(), "Simulate".into(), "Validate".into(), "SwitchMode".into()],
                    weight: 0.6,
                    usage_count: 0,
                    avg_success: 0.5,
                },
            ],
            version: 0,
        }
    }

    /// Sélectionner la meilleure règle pour le contexte actuel
    pub fn select_rule(&self, is_stagnating: bool) -> &GrammarRule {
        if is_stagnating {
            // Préférer DEEP_EVOLVE ou SAFE_EVOLVE en stagnation
            self.grammar_rules
                .iter()
                .filter(|r| r.pattern == "DEEP_EVOLVE" || r.pattern == "SAFE_EVOLVE")
                .max_by(|a, b| a.avg_success.partial_cmp(&b.avg_success).unwrap())
                .unwrap_or(&self.grammar_rules[0])
        } else {
            // Sélection par poids * succès
            self.grammar_rules
                .iter()
                .max_by(|a, b| {
                    let score_a = a.weight * a.avg_success;
                    let score_b = b.weight * b.avg_success;
                    score_a.partial_cmp(&score_b).unwrap()
                })
                .unwrap()
        }
    }

    /// Expand une règle en programme IL exécutable
    pub fn expand_to_program(&self, rule: &GrammarRule, operands: &[&str]) -> ILProgram {
        let mut program = ILProgram::new(&rule.pattern);
        program.push(ILInstruction::new(OpCode::Init, vec![&rule.pattern]));

        for step_name in &rule.expansion {
            let opcode = match step_name.as_str() {
                "Search" => OpCode::Search,
                "Embed" => OpCode::Embed,
                "Rank" => OpCode::Rank,
                "Filter" => OpCode::Filter,
                "Store" => OpCode::Store,
                "Eval" => OpCode::Eval,
                "Load" => OpCode::Load,
                "Mutate" => OpCode::Mutate,
                "Simulate" => OpCode::Simulate,
                "Validate" => OpCode::Validate,
                "Commit" => OpCode::Commit,
                "Rollback" => OpCode::Rollback,
                "Observe" => OpCode::Observe,
                "Identify" => OpCode::Identify,
                "Propose" => OpCode::Propose,
                "Apply" => OpCode::Apply,
                "SwitchMode" => OpCode::SwitchMode,
                _ => OpCode::Nop,
            };
            program.push(ILInstruction::new(opcode, operands.to_vec()));
        }

        program.push(ILInstruction::new(OpCode::Eval, vec!["fitness"]));
        program
    }

    /// Mettre à jour le succès d'une règle après exécution
    pub fn report_execution(&mut self, pattern: &str, success: bool) {
        if let Some(rule) = self.grammar_rules.iter_mut().find(|r| r.pattern == pattern) {
            rule.usage_count += 1;
            let outcome = if success { 1.0 } else { 0.0 };
            // Moyenne mobile exponentielle
            rule.avg_success = rule.avg_success * 0.9 + outcome * 0.1;
            // Ajuster le poids
            rule.weight = (rule.weight + (rule.avg_success - 0.5) * 0.05).clamp(0.1, 2.0);
        }
    }

    /// Faire évoluer la grammaire
    pub fn evolve_grammar(&mut self) {
        let mut rng = rand::thread_rng();

        // Mutation rare : ajouter/retirer un step
        for rule in &mut self.grammar_rules {
            if rng.gen_bool(0.05) && rule.expansion.len() < 8 {
                let options = ["Validate", "Simulate", "Store", "Embed", "Observe"];
                let new_step = options[rng.gen_range(0..options.len())];
                let pos = rng.gen_range(0..=rule.expansion.len());
                rule.expansion.insert(pos, new_step.to_string());
            }
            if rng.gen_bool(0.03) && rule.expansion.len() > 2 {
                let pos = rng.gen_range(0..rule.expansion.len());
                rule.expansion.remove(pos);
            }
        }

        self.version += 1;
    }

    pub fn available_patterns(&self) -> Vec<String> {
        self.grammar_rules.iter().map(|r| r.pattern.clone()).collect()
    }
}
