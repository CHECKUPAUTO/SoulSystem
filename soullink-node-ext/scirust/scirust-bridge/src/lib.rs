use regex::Regex;
use scirust_learning::PatternMemory;
use scirust_reasoning::{solve_linear, solve_quadratic, apply_trig_identity, prove_equal};
use scirust_symbolic::{Expr, diff, eval, parse, simplify, to_rust_code};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// NLP Parser: natural language → Expr or command
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum NaturalCommand {
    Simplify(Expr),
    Derivative(Expr, String),
    Solve(Expr, String),
    Evaluate(Expr, HashMap<String, f64>),
    TrigSimplify(Expr),
    ProveEqual(Expr, Expr),
    Raw(Expr),
    /// Bayesian inference query
    Infer { query: String, network: String, evidence: Vec<(String, f64)> },
    /// Check logical consistency
    CheckConsistency { propositions: Vec<String> },
    /// Explain inference for a node
    Explain { node: String },
}

pub fn parse_natural(input: &str) -> Result<NaturalCommand, String> {
    let lower = input.to_lowercase().trim().to_string();

    // --- Derivative patterns ---
    let deriv_patterns = [
        r"^(dérivée|derivée|derivative|d/dx|diff|derive)\s+(de\s+|of\s+)?(.+)",
        r"^(dérivée|derivée|derivative|d/dx|diff|derive)\s+(.+)\s+par\s+rapport\s+à\s+(\w+)",
        r"^(dérivée|derivée|derivative|d/dx|diff|derive)\s+(.+)\s+w\.r\.t\.?\s+(\w+)",
    ];
    for pat in &deriv_patterns {
        if let Ok(re) = Regex::new(pat) {
            if let Some(caps) = re.captures(&lower) {
                let expr_str = caps.get(3).unwrap().as_str().trim();
                let expr = parse(expr_str)?;
                let var = caps.get(4).map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| extract_variable(&expr).unwrap_or_else(|| "x".to_string()));
                return Ok(NaturalCommand::Derivative(expr, var));
            }
        }
    }

    // --- Simplify patterns ---
    let simp_patterns = [
        r"^(simplifie|simplify|simp|réduis|reduce)\s+(.+)",
        r"^factorise\s+(.+)",
        r"^développe\s+(.+)",
    ];
    for pat in &simp_patterns {
        if let Ok(re) = Regex::new(pat) {
            if let Some(caps) = re.captures(&lower) {
                let expr_str = caps.get(2).unwrap().as_str().trim();
                let expr = parse(expr_str)?;
                return Ok(NaturalCommand::Simplify(expr));
            }
        }
    }

    // --- Solve patterns ---
    let solve_patterns = [
        r"^(résous|resous|solve|résolution|resolution)\s+(.+)\s*=\s*0",
        r"^(résous|resous|solve)\s+(.+)\s+pour\s+(\w+)",
        r"^(racines?|roots?)\s+de\s+(.+)",
    ];
    for pat in &solve_patterns {
        if let Ok(re) = Regex::new(pat) {
            if let Some(caps) = re.captures(&lower) {
                let expr_str = if caps.len() == 4 { caps.get(2).unwrap().as_str().trim() } else { caps.get(2).unwrap().as_str().trim() };
                let expr = parse(expr_str)?;
                let var = caps.get(3).map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| extract_variable(&expr).unwrap_or_else(|| "x".to_string()));
                return Ok(NaturalCommand::Solve(expr, var));
            }
        }
    }

    // --- Trig simplify ---
    if let Ok(re) = Regex::new(r"^(trig|trig_simp|trig_simplify|trig_simplifie)\s+(.+)") {
        if let Some(caps) = re.captures(&lower) {
            let expr_str = caps.get(2).unwrap().as_str().trim();
            let expr = parse(expr_str)?;
            return Ok(NaturalCommand::TrigSimplify(expr));
        }
    }

    // --- Prove equality ---
    if let Ok(re) = Regex::new(r"^(prouve|prove|vérifie|verify)\s+(.+)\s*=\s*(.+)") {
        if let Some(caps) = re.captures(&lower) {
            let left_str = caps.get(2).unwrap().as_str().trim();
            let right_str = caps.get(3).unwrap().as_str().trim();
            let left = parse(left_str)?;
            let right = parse(right_str)?;
            return Ok(NaturalCommand::ProveEqual(left, right));
        }
    }

    // --- Evaluate patterns ---
    if let Ok(re) = Regex::new(r"^(évalue|eval|evaluate|calcule|compute)\s+(.+)") {
        if let Some(caps) = re.captures(&lower) {
            let expr_str = caps.get(2).unwrap().as_str().trim();
            let expr = parse(expr_str)?;
            return Ok(NaturalCommand::Evaluate(expr, HashMap::new()));
        }
    }

    // Default: try direct math parsing
    let expr = parse(input)?;
    Ok(NaturalCommand::Raw(expr))
}

fn extract_variable(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Var(name) => Some(name.clone()),
        Expr::Add(a, _) |
        Expr::Sub(a, _) |
        Expr::Mul(a, _) |
        Expr::Div(a, _) |
        Expr::Pow(a, _) => extract_variable(a),
        Expr::Sin(a) |
        Expr::Cos(a) |
        Expr::Exp(a) |
        Expr::Ln(a) |
        Expr::Sqrt(a) |
        Expr::Abs(a) |
        Expr::Neg(a) => extract_variable(a),
        Expr::Const(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Pipeline Orchestrator
// ---------------------------------------------------------------------------

pub struct Pipeline {
    pub memory: PatternMemory,
    pub vars: HashMap<String, f64>,
}

/// Parse evidence string like "x=1.0, y=2.5" into Vec<(String, f64)>
fn parse_evidence(input: &str) -> Vec<(String, f64)> {
    input.split(',').filter_map(|pair| {
        let parts: Vec<&str> = pair.trim().splitn(2, '=').collect();
        if parts.len() == 2 {
            let name = parts[0].trim().to_string();
            if let Ok(val) = parts[1].trim().parse::<f64>() {
                return Some((name, val));
            }
        }
        None
    }).collect()
}

impl Pipeline {
    pub fn new() -> Self {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), 2.0);
        vars.insert("y".to_string(), 3.0);
        vars.insert("z".to_string(), 1.0);
        Self {
            memory: PatternMemory::new(),
            vars,
        }
    }

    pub fn with_vars(mut self, vars: HashMap<String, f64>) -> Self {
        self.vars = vars;
        self
    }

    pub fn run(&mut self,
        input: &str,
    ) -> Result<PipelineOutput, String> {
        let cmd = parse_natural(input)?;
        self.run_command(cmd)
    }

    pub fn run_command(&mut self,
        cmd: NaturalCommand,
    ) -> Result<PipelineOutput, String> {
        let (expr, action) = match &cmd {
            NaturalCommand::Simplify(e) => (e.clone(), "simplify"),
            NaturalCommand::Derivative(e, _) => (e.clone(), "derivative"),
            NaturalCommand::Solve(e, _) => (e.clone(), "solve"),
            NaturalCommand::Evaluate(e, _) => (e.clone(), "evaluate"),
            NaturalCommand::TrigSimplify(e) => (e.clone(), "trig_simplify"),
            NaturalCommand::ProveEqual(l, r) => return Ok(self.prove(l, r)),
            NaturalCommand::Raw(e) => (e.clone(), "raw"),
            NaturalCommand::Infer { query, .. } => (parse(query).unwrap_or(Expr::Const(0.0)), "infer"),
            NaturalCommand::CheckConsistency { .. } => (Expr::Const(0.0), "check"),
            NaturalCommand::Explain { .. } => (Expr::Const(0.0), "explain"),
        };

        let parsed_str = format!("{}", expr);

        // Step 1: Simplify
        let simplified = simplify(&expr);
        let simplified_str = format!("{}", simplified);

        // Step 2: Compute derivative if applicable
        let var = extract_variable(&simplified).unwrap_or_else(|| "x".to_string());
        let derivative = if matches!(cmd, NaturalCommand::Derivative(_, _)) {
            diff(&simplified, &var)
        } else {
            diff(&simplified, &var)
        };
        let derivative_simplified = simplify(&derivative);
        let derivative_str = format!("{}", derivative_simplified);

        // Step 3: Apply trig identities if requested
        let trig_simplified = if action == "trig_simplify" {
            let ts = apply_trig_identity(&simplified);
            format!("{}", ts)
        } else {
            simplified_str.clone()
        };

        // Step 4: Solve if applicable
        let solution_str = if action == "solve" {
            let roots = solve_quadratic(&simplified, &var);
            if roots.is_empty() {
                solve_linear(&simplified, &var)
                    .map(|r| format!("{}", r))
                    .unwrap_or_else(|| "no closed-form root found".to_string())
            } else {
                format!("{:?}", roots)
            }
        } else {
            String::new()
        };

        // Step 5: Evaluate numerically
        let value = eval(&simplified, &self.vars)
            .unwrap_or(f64::NAN);

        // Step 6: Generate Rust code
        let vars_list: Vec<&str> = self.vars.keys().map(|s| s.as_str()).collect();
        let code = to_rust_code(&simplified, &vars_list);

        // Step 7: Record in memory
        self.memory.record(&expr, &simplified, !value.is_nan());

        Ok(PipelineOutput {
            parsed: parsed_str,
            simplified: if action == "trig_simplify" { trig_simplified } else { simplified_str },
            derivative: derivative_str,
            value,
            rust_code: code,
            solution: solution_str,
            action: action.to_string(),
        })
    }

    fn prove(&mut self, left: &Expr, right: &Expr) -> PipelineOutput {
        let equal = prove_equal(left, right);
        PipelineOutput {
            parsed: format!("{} = {}", left, right),
            simplified: if equal { "PROVEN EQUAL".to_string() } else { "NOT PROVEN".to_string() },
            derivative: String::new(),
            value: if equal { 1.0 } else { 0.0 },
            rust_code: String::new(),
            solution: String::new(),
            action: "prove".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineOutput {
    pub parsed: String,
    pub simplified: String,
    pub derivative: String,
    pub value: f64,
    pub rust_code: String,
    pub solution: String,
    pub action: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_derivative_fr() {
        let cmd = parse_natural("dérivée de x^2").unwrap();
        assert!(matches!(cmd, NaturalCommand::Derivative(_, _)));
    }

    #[test]
    fn test_parse_solve() {
        let cmd = parse_natural("solve x^2 - 5*x + 6 = 0").unwrap();
        assert!(matches!(cmd, NaturalCommand::Solve(_, _)));
    }

    #[test]
    fn test_parse_prove() {
        let cmd = parse_natural("prove x + x = 2*x").unwrap();
        assert!(matches!(cmd, NaturalCommand::ProveEqual(_, _)));
    }

    #[test]
    fn test_pipeline_solve() {
        let mut p = Pipeline::new();
        let out = p.run("solve x^2 - 5*x + 6 = 0").unwrap();
        assert!(!out.solution.is_empty());
    }
}
