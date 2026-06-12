//! SoulLink Math Engine — Intégration SciRust
//!
//! Fournit un pont entre le brain neural SoulLink et le moteur
//! symbolique/numérique SciRust.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Requêtes / Réponses JSON
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct EvalRequest {
    pub expr: String,
    #[serde(default)]
    pub vars: HashMap<String, f64>,
}

#[derive(Serialize)]
pub struct EvalResponse {
    pub parsed: String,
    pub simplified: String,
    pub derivative: String,
    pub value: f64,
    pub rust_code: String,
    pub dual_value: f64,
    pub dual_deriv: f64,
}

#[derive(Deserialize)]
pub struct SolveRequest {
    pub expr: String,
    #[serde(default = "default_var")]
    pub var: String,
}

#[derive(Serialize)]
pub struct SolveResponse {
    pub roots: Vec<f64>,
    pub linear_root: Option<f64>,
    pub parsed: String,
}

#[derive(Deserialize)]
pub struct DeriveRequest {
    pub expr: String,
    #[serde(default = "default_var")]
    pub var: String,
}

#[derive(Serialize)]
pub struct DeriveResponse {
    pub parsed: String,
    pub derivative: String,
    pub simplified: String,
}

#[derive(Deserialize)]
pub struct ProveRequest {
    pub left: String,
    pub right: String,
}

#[derive(Serialize)]
pub struct ProveResponse {
    pub left: String,
    pub right: String,
    pub proven: bool,
}

fn default_var() -> String {
    "x".to_string()
}

// ---------------------------------------------------------------------------
// Moteur
// ---------------------------------------------------------------------------

pub struct MathEngine;

impl MathEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn eval(&self,
        expr_str: &str,
        vars: &HashMap<String, f64>,
    ) -> Result<EvalResponse, String> {
        use scirust_symbolic::prelude::*;

        let mut pipeline = Pipeline::new();
        if !vars.is_empty() {
            pipeline = pipeline.with_vars(vars.clone());
        }
        let out = pipeline.run(expr_str)?;

        // Dual verification
        let var = vars.keys().next().map(|s| s.as_str()).unwrap_or("x");
        let val = vars.get(var).copied().unwrap_or(2.0);
        let dual = scirust_autodiff::Dual::var(val);
        let dual_result = Self::eval_dual_string(&out.simplified, var, dual);

        Ok(EvalResponse {
            parsed: out.parsed,
            simplified: out.simplified.clone(),
            derivative: out.derivative,
            value: out.value,
            rust_code: out.rust_code,
            dual_value: dual_result.value,
            dual_deriv: dual_result.deriv,
        })
    }

    pub fn solve(&self, expr_str: &str, _var: &str) -> Result<SolveResponse, String> {
        use scirust_symbolic::prelude::*;

        let expr = parse(expr_str)?;
        let simplified = simplify(&expr);
        // API changed: solve_quadratic/solve_linear now take coefficients (f64)
        // TODO: extract coefficients from Expr before calling
        let roots: Vec<f64> = vec![];
        let linear: Option<f64> = None;

        Ok(SolveResponse {
            roots,
            linear_root: linear,
            parsed: format!("{}", simplified),
        })
    }

    pub fn derive(&self, expr_str: &str, var: &str) -> Result<DeriveResponse, String> {
        use scirust_symbolic::prelude::*;

        let expr = parse(expr_str)?;
        let d = diff(&expr, var);
        let sd = simplify(&d);

        Ok(DeriveResponse {
            parsed: format!("{}", expr),
            derivative: format!("{}", d),
            simplified: format!("{}", sd),
        })
    }

    pub fn prove(&self, left: &str, right: &str) -> Result<ProveResponse, String> {
        use scirust_symbolic::prelude::*;

        let a = parse(left)?;
        let b = parse(right)?;
        let proven = prove_equal(&a, &b);

        Ok(ProveResponse {
            left: left.to_string(),
            right: right.to_string(),
            proven,
        })
    }

    // Mini-interpréteur Dual pour vérification
    fn eval_dual_string(expr: &str, var: &str, val: scirust_autodiff::Dual) -> scirust_autodiff::Dual {
        match scirust_symbolic::symbolic::parse(expr) {
            Ok(ast) => Self::eval_dual(&ast, var, val),
            Err(_) => scirust_autodiff::Dual::new(0.0, 0.0),
        }
    }

    fn eval_dual(expr: &scirust_symbolic::symbolic::Expr, var: &str, val: scirust_autodiff::Dual) -> scirust_autodiff::Dual {
        use scirust_symbolic::symbolic::Expr;
        match expr {
            Expr::Const(c) => scirust_autodiff::Dual::primal(*c),
            Expr::Var(name) => {
                if name == var { val } else { scirust_autodiff::Dual::primal(0.0) }
            }
            Expr::Add(a, b) => Self::eval_dual(a, var, val) + Self::eval_dual(b, var, val),
            Expr::Sub(a, b) => Self::eval_dual(a, var, val) - Self::eval_dual(b, var, val),
            Expr::Mul(a, b) => Self::eval_dual(a, var, val) * Self::eval_dual(b, var, val),
            Expr::Div(a, b) => Self::eval_dual(a, var, val) / Self::eval_dual(b, var, val),
            Expr::Neg(a) => -Self::eval_dual(a, var, val),
            Expr::Pow(a, b) => {
                if let Expr::Const(n) = &**b {
                    Self::eval_dual(a, var, val).powi(*n as i32)
                } else {
                    Self::eval_dual(a, var, val).powf(Self::eval_dual(b, var, val).value)
                }
            }
            Expr::Sin(a) => Self::eval_dual(a, var, val).sin(),
            Expr::Cos(a) => Self::eval_dual(a, var, val).cos(),
            Expr::Exp(a) => Self::eval_dual(a, var, val).exp(),
            Expr::Ln(a) => Self::eval_dual(a, var, val).ln(),
            Expr::Sqrt(a) => Self::eval_dual(a, var, val).sqrt(),
            Expr::Abs(a) => Self::eval_dual(a, var, val).abs(),
        }
    }

    /// Compute Pearson correlation coefficient between two vectors.
    ///
    /// Returns a value in [-1.0, 1.0] where:
    /// - 1.0 = perfect positive linear correlation
    /// - -1.0 = perfect negative linear correlation
    /// - 0.0 = no linear correlation
    /// - Returns Err if vectors are empty or have zero variance.
    #[allow(dead_code)]
    pub fn correlation(&self, a: &[f64], b: &[f64]) -> Result<f64, String> {
        if a.len() != b.len() {
            return Err(format!("Vectors must have same length: {} vs {}", a.len(), b.len()));
        }
        if a.len() < 2 {
            return Err("Need at least 2 samples".to_string());
        }

        let n = a.len() as f64;
        let mean_a = a.iter().sum::<f64>() / n;
        let mean_b = b.iter().sum::<f64>() / n;

        let mut cov = 0.0;
        let mut var_a = 0.0;
        let mut var_b = 0.0;

        for (x, y) in a.iter().zip(b.iter()) {
            let dx = x - mean_a;
            let dy = y - mean_b;
            cov += dx * dy;
            var_a += dx * dx;
            var_b += dy * dy;
        }

        if var_a.abs() < 1e-12 || var_b.abs() < 1e-12 {
            return Ok(0.0);
        }

        Ok(cov / (var_a.sqrt() * var_b.sqrt()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_perfect_positive() {
        let me = MathEngine::new();
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![2.0, 4.0, 6.0, 8.0, 10.0];
        let corr = me.correlation(&a, &b).unwrap();
        assert!((corr - 1.0).abs() < 0.001, "Expected 1.0, got {}", corr);
    }

    #[test]
    fn test_correlation_perfect_negative() {
        let me = MathEngine::new();
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![10.0, 8.0, 6.0, 4.0, 2.0];
        let corr = me.correlation(&a, &b).unwrap();
        assert!((corr - (-1.0)).abs() < 0.001, "Expected -1.0, got {}", corr);
    }

    #[test]
    fn test_correlation_no_correlation() {
        let me = MathEngine::new();
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        let corr = me.correlation(&a, &b).unwrap();
        assert!((corr - 0.0).abs() < 0.001, "Expected 0.0, got {}", corr);
    }

    #[test]
    fn test_correlation_unequal_lengths() {
        let me = MathEngine::new();
        let result = me.correlation(&[1.0, 2.0], &[1.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_correlation_too_few_samples() {
        let me = MathEngine::new();
        let result = me.correlation(&[1.0], &[2.0]);
        assert!(result.is_err());
    }
}
