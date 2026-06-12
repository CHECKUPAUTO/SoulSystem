//! Détecteur symbolique — repère les expressions mathématiques équivalentes
//! entre différents projets et signale les doublons potentiels.

use crate::detect::{DetectContext, Detector};
use crate::types::Synergy;
use anyhow::Result;

pub struct SymbolicDetector;

impl Detector for SymbolicDetector {
    fn name(&self) -> &'static str {
        "symbolic"
    }

    fn detect(&self, ctx: &DetectContext) -> Result<Vec<Synergy>> {
        let mut synergies = Vec::new();

        if ctx.expressions.is_empty() {
            return Ok(synergies);
        }

        // Reconstruire les expressions parsées par projet
        let mut parsed: Vec<(String, Vec<(String, scirust_symbolic::Expr)>)> = Vec::new();

        for pe in &ctx.expressions {
            let mut entries: Vec<(String, scirust_symbolic::Expr)> = Vec::new();
            for expr_str in &pe.expressions {
                let cleaned = clean_expression(expr_str);
                match scirust_symbolic::parse(&cleaned) {
                    Ok(e) => entries.push((cleaned, e)),
                    Err(_) => continue,
                }
            }
            if !entries.is_empty() {
                parsed.push((pe.project_name.clone(), entries));
            }
        }

        for i in 0..parsed.len() {
            for j in i + 1..parsed.len() {
                let (p1_name, p1_exprs) = &parsed[i];
                let (p2_name, p2_exprs) = &parsed[j];

                for (s1_str, e1) in p1_exprs {
                    for (s2_str, e2) in p2_exprs {
                        if scirust_symbolic::prove_equal(e1, e2) {
                            let score = compute_score(e1, 2);
                            let description = format!(
                                "Expression équivalente entre '{}' et '{}' : `{}` ↔ `{}`",
                                p1_name, p2_name, s1_str, s2_str
                            );
                            synergies.push(Synergy {
                                id: format!(
                                    "symbolic-{}-{}-{}",
                                    p1_name, p2_name, hash_expr(s1_str)
                                ),
                                synergy_type: "Deduplication".into(),
                                source_project: p1_name.clone(),
                                target_project: p2_name.clone(),
                                description,
                                priority: score,
                                effort: "medium".into(),
                                value: score_to_value(score),
                                auto_score: score as f32,
                                adjusted_score: score as f32,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }

        Ok(synergies)
    }
}

fn clean_expression(input: &str) -> String {
    let trimmed = input.trim();
    if let Some(eq_pos) = trimmed.find('=') {
        let right = &trimmed[eq_pos + 1..].trim();
        let left = &trimmed[..eq_pos].trim();
        if left.contains('(')
            || left.contains("let")
            || left.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
        {
            return right.to_string();
        }
    }
    trimmed.to_string()
}

fn compute_score(expr: &scirust_symbolic::Expr, project_count: usize) -> u8 {
    let depth = expr_depth(expr);
    let raw = depth.min(10) as f64 / 10.0;
    let proj_factor = (project_count as f64).min(5.0) / 5.0;
    let combined = (raw * 0.6 + proj_factor * 0.4) * 10.0;
    (combined.round() as u8).max(1).min(10)
}

fn score_to_value(score: u8) -> String {
    match score {
        8..=10 => "high".into(),
        4..=7 => "medium".into(),
        _ => "low".into(),
    }
}

fn expr_depth(expr: &scirust_symbolic::Expr) -> usize {
    use scirust_symbolic::Expr;
    match expr {
        Expr::Const(_) | Expr::Var(_) => 1,
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::Pow(a, b) => 1 + expr_depth(a).max(expr_depth(b)),
        Expr::Neg(a)
        | Expr::Sin(a)
        | Expr::Cos(a)
        | Expr::Exp(a)
        | Expr::Ln(a)
        | Expr::Sqrt(a)
        | Expr::Abs(a) => 1 + expr_depth(a),
    }
}

fn hash_expr(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
