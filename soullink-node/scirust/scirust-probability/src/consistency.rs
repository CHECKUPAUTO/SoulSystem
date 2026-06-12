//! Consistency engine for logical rules and propositions.

use scirust_reasoning::prove_equal;
use scirust_symbolic::Expr;
use std::collections::{HashMap, HashSet, VecDeque};

// ---------------------------------------------------------------------------
// Rule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Rule {
    pub antecedent: Expr,
    pub consequent: Expr,
    pub confidence: f64,
}

impl Rule {
    pub fn implies(antecedent: Expr, consequent: Expr, confidence: f64) -> Self {
        Self { antecedent, consequent, confidence: confidence.clamp(0.0, 1.0) }
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} → {} (conf: {:.2})", self.antecedent, self.consequent, self.confidence)
    }
}

// ---------------------------------------------------------------------------
// ConsistencyScore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConsistencyScore {
    pub score: f64,
    pub contradiction_count: usize,
    pub cycle_count: usize,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

impl ConsistencyScore {
    pub fn perfect() -> Self {
        Self { score: 1.0, contradiction_count: 0, cycle_count: 0, issues: Vec::new(), suggestions: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// ConsistencyEngine
// ---------------------------------------------------------------------------

pub struct ConsistencyEngine {
    rules: Vec<Rule>,
    facts: Vec<Expr>,
}

impl ConsistencyEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new(), facts: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: Rule) { self.rules.push(rule); }
    pub fn add_fact(&mut self, fact: Expr) { self.facts.push(fact); }
    pub fn rules(&self) -> &[Rule] { &self.rules }
    pub fn facts(&self) -> &[Expr] { &self.facts }

    pub fn check_direct_contradictions(&self) -> Vec<String> {
        let mut issues = Vec::new();
        for i in 0..self.facts.len() {
            for j in (i + 1)..self.facts.len() {
                if is_negation_of(&self.facts[i], &self.facts[j]) {
                    issues.push(format!(
                        "Direct contradiction: '{}' vs '{}'",
                        self.facts[i], self.facts[j]
                    ));
                }
            }
        }
        issues
    }

    pub fn check_rule_contradictions(&self) -> Vec<String> {
        let mut issues = Vec::new();
        for i in 0..self.rules.len() {
            for j in (i + 1)..self.rules.len() {
                if prove_equal(&self.rules[i].antecedent, &self.rules[j].antecedent) {
                    if is_negation_of(&self.rules[i].consequent, &self.rules[j].consequent) {
                        let combined_conf = self.rules[i].confidence * self.rules[j].confidence;
                        issues.push(format!(
                            "Rule contradiction: '{}' and '{}' (combined conf: {:.2})",
                            self.rules[i], self.rules[j], combined_conf
                        ));
                    }
                }
            }
        }
        issues
    }

    pub fn check_indirect_contradictions(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let rule_graph = self.build_rule_graph();

        for i in 0..self.rules.len() {
            let mut reachable: HashMap<String, Vec<(f64, Vec<usize>)>> = HashMap::new();
            let mut queue = VecDeque::new();
            queue.push_back((&self.rules[i].consequent, self.rules[i].confidence, vec![i]));

            while let Some((expr, conf, path)) = queue.pop_front() {
                let key = format!("{}", expr);
                reachable.entry(key.clone()).or_default().push((conf, path.clone()));

                if let Some(edges) = rule_graph.get(&key) {
                    for (next_expr, edge_conf, rule_idx) in edges {
                        if !path.contains(rule_idx) {
                            let new_conf = conf * edge_conf;
                            let mut new_path = path.clone();
                            new_path.push(*rule_idx);
                            queue.push_back((next_expr, new_conf, new_path));
                        }
                    }
                }
            }

            let nodes: Vec<(String, Vec<(f64, Vec<usize>)>)> = reachable.into_iter().collect();
            for a in 0..nodes.len() {
                for b in (a + 1)..nodes.len() {
                    if let (Ok(ea), Ok(eb)) = (
                        scirust_symbolic::parse(&nodes[a].0),
                        scirust_symbolic::parse(&nodes[b].0),
                    ) {
                        if is_negation_of(&ea, &eb) {
                            let max_conf_a = nodes[a].1.iter().map(|(c, _)| *c).fold(0.0f64, f64::max);
                            let max_conf_b = nodes[b].1.iter().map(|(c, _)| *c).fold(0.0f64, f64::max);
                            issues.push(format!(
                                "Indirect contradiction: from '{}' reach '{}' (conf: {:.2}) and '{}' (conf: {:.2})",
                                self.rules[i].antecedent, nodes[a].0, max_conf_a, nodes[b].0, max_conf_b
                            ));
                        }
                    }
                }
            }
        }
        issues
    }

    pub fn check_cycles(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let rule_graph = self.build_rule_graph();

        for i in 0..self.rules.len() {
            let target = format!("{}", self.rules[i].antecedent);
            let start_expr = format!("{}", self.rules[i].consequent);

            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();
            queue.push_back(start_expr.clone());

            while let Some(current) = queue.pop_front() {
                if current == target {
                    let min_conf = self.rules.iter().map(|r| r.confidence).fold(1.0f64, |a, b| a * b);
                    if min_conf > 0.3 {
                        issues.push(format!(
                            "Logical cycle detected: '{}' leads back to '{}' (path conf: {:.2})",
                            self.rules[i], self.rules[i].antecedent, min_conf
                        ));
                    }
                    break;
                }
                if visited.contains(&current) { continue; }
                visited.insert(current.clone());

                if let Some(edges) = rule_graph.get(&current) {
                    for (next_expr, _, _) in edges {
                        let nk = format!("{}", next_expr);
                        if !visited.contains(&nk) {
                            queue.push_back(nk);
                        }
                    }
                }
            }
        }
        issues
    }

    fn build_rule_graph(&self) -> HashMap<String, Vec<(Expr, f64, usize)>> {
        let mut graph: HashMap<String, Vec<(Expr, f64, usize)>> = HashMap::new();
        for (i, rule) in self.rules.iter().enumerate() {
            let key = format!("{}", rule.antecedent);
            graph.entry(key).or_default().push((
                rule.consequent.clone(),
                rule.confidence,
                i,
            ));
        }
        graph
    }

    pub fn check(&self, propositions: &[Expr]) -> ConsistencyScore {
        let mut issues = Vec::new();

        // Check facts against propositions
        for prop in propositions {
            for fact in &self.facts {
                if is_negation_of(prop, fact) {
                    issues.push(format!("Proposition '{}' contradicts fact '{}'", prop, fact));
                }
            }
        }

        // Check against rules
        for prop in propositions {
            for rule in &self.rules {
                if is_negation_of(prop, &rule.consequent) {
                    issues.push(format!(
                        "Proposition '{}' contradicts consequence of rule '{}'",
                        prop, rule
                    ));
                }
            }
        }

        let direct = self.check_direct_contradictions();
        let rule_contra = self.check_rule_contradictions();
        let indirect = self.check_indirect_contradictions();
        let cycles = self.check_cycles();

        let all_issues_count = issues.len() + direct.len() + rule_contra.len() + indirect.len() + cycles.len();
        issues.extend(direct);
        issues.extend(rule_contra);
        issues.extend(indirect);
        issues.extend(cycles.clone());

        let score = if all_issues_count == 0 {
            1.0
        } else {
            let penalty = all_issues_count as f64 * 0.2;
            (1.0 - penalty).max(0.0)
        };

        let mut suggestions = Vec::new();
        if !issues.is_empty() {
            suggestions.push("Review rules with lowest confidence for potential relaxation.".to_string());
        }

        ConsistencyScore {
            score,
            contradiction_count: all_issues_count,
            cycle_count: cycles.len(),
            issues,
            suggestions,
        }
    }

    pub fn suggest_resolution(&self, score: &ConsistencyScore) -> Vec<String> {
        let mut suggestions = Vec::new();
        if score.contradiction_count > 0 {
            let mut rules_with_conf: Vec<(usize, f64)> = self.rules.iter()
                .enumerate()
                .map(|(i, r)| (i, r.confidence))
                .collect();
            rules_with_conf.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            if let Some((idx, conf)) = rules_with_conf.first() {
                suggestions.push(format!(
                    "Relax rule #{}: '{}' (currently confidence {:.2})",
                    idx, self.rules[*idx], conf
                ));
            }
            suggestions.push("Consider removing or revising the most recently added fact.".to_string());
        }
        suggestions
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_negation_of(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Neg(inner), other) | (other, Expr::Neg(inner)) => {
            prove_equal(inner, other)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use scirust_symbolic::parse;

    #[test]
    fn test_rule_creation() {
        let a = parse("x").unwrap();
        let b = parse("y").unwrap();
        let rule = Rule::implies(a, b, 0.9);
        assert!((rule.confidence - 0.9).abs() < 1e-10);
    }

    #[test]
    fn test_direct_contradiction() {
        let mut engine = ConsistencyEngine::new();
        engine.add_fact(parse("x").unwrap());
        let score = engine.check(&[parse("-x").unwrap()]);
        assert!(score.score < 1.0);
    }

    #[test]
    fn test_rule_contradiction_detection() {
        let mut engine = ConsistencyEngine::new();
        engine.add_rule(Rule::implies(parse("x").unwrap(), parse("y").unwrap(), 0.9));
        engine.add_rule(Rule::implies(parse("x").unwrap(), parse("-y").unwrap(), 0.8));
        let issues = engine.check_rule_contradictions();
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_cycle_detection() {
        let mut engine = ConsistencyEngine::new();
        engine.add_rule(Rule::implies(parse("a").unwrap(), parse("b").unwrap(), 0.8));
        engine.add_rule(Rule::implies(parse("b").unwrap(), parse("c").unwrap(), 0.8));
        engine.add_rule(Rule::implies(parse("c").unwrap(), parse("a").unwrap(), 0.8));
        let issues = engine.check_cycles();
        assert!(!issues.is_empty(), "Should detect cycle A→B→C→A");
    }

    #[test]
    fn test_perfect_consistency() {
        let engine = ConsistencyEngine::new();
        let score = engine.check(&[]);
        assert!((score.score - 1.0).abs() < 1e-10);
        assert_eq!(score.contradiction_count, 0);
    }

    #[test]
    fn test_suggest_resolution() {
        let mut engine = ConsistencyEngine::new();
        engine.add_rule(Rule::implies(parse("x").unwrap(), parse("y").unwrap(), 0.5));
        engine.add_rule(Rule::implies(parse("x").unwrap(), parse("-y").unwrap(), 0.9));
        let score = engine.check(&[]);
        let suggestions = engine.suggest_resolution(&score);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_indirect_contradiction_chain() {
        let mut engine = ConsistencyEngine::new();
        engine.add_rule(Rule::implies(parse("a").unwrap(), parse("b").unwrap(), 0.9));
        engine.add_rule(Rule::implies(parse("b").unwrap(), parse("c").unwrap(), 0.8));
        engine.add_rule(Rule::implies(parse("b").unwrap(), parse("-c").unwrap(), 0.7));
        let score = engine.check(&[parse("a").unwrap()]);
        assert!(score.contradiction_count > 0 || score.score < 1.0);
    }
}
