//! Equation solvers and optimizer.

use crate::symbolic::Expr;

/// Solve a linear equation `expr = 0` for variable `var`.
/// Assumes `expr` is of the form `a*var + b`.
pub fn solve_linear(expr: &Expr, var: &str) -> Option<f64> {
    let (a, b) = extract_linear_coeffs(expr, var)?;
    if a.abs() < 1e-15 {
        None // no solution or infinite
    } else {
        Some(-b / a)
    }
}

fn extract_linear_coeffs(expr: &Expr, var: &str) -> Option<(f64, f64)> {
    match expr {
        Expr::Const(c) => Some((0.0, *c)),
        Expr::Var(v) if v == var => Some((1.0, 0.0)),
        Expr::Var(_) => Some((0.0, 0.0)),
        Expr::Add(a, b) => {
            let (a1, b1) = extract_linear_coeffs(a, var)?;
            let (a2, b2) = extract_linear_coeffs(b, var)?;
            Some((a1 + a2, b1 + b2))
        }
        Expr::Sub(a, b) => {
            let (a1, b1) = extract_linear_coeffs(a, var)?;
            let (a2, b2) = extract_linear_coeffs(b, var)?;
            Some((a1 - a2, b1 - b2))
        }
        Expr::Mul(a, b) => {
            match (a.as_ref(), b.as_ref()) {
                (&Expr::Const(coeff), Expr::Var(v)) if v == var => Some((coeff, 0.0)),
                (Expr::Var(v), &Expr::Const(coeff)) if v == var => Some((coeff, 0.0)),
                (&Expr::Const(c), inner) => {
                    let (a_coeff, b_coeff) = extract_linear_coeffs(inner, var)?;
                    Some((a_coeff * c, b_coeff * c))
                }
                (inner, &Expr::Const(c)) => {
                    let (a_coeff, b_coeff) = extract_linear_coeffs(inner, var)?;
                    Some((a_coeff * c, b_coeff * c))
                }
                _ => None, // non-linear
            }
        }
        Expr::Neg(a) => {
            let (a1, b1) = extract_linear_coeffs(a, var)?;
            Some((-a1, -b1))
        }
        _ => None, // non-linear
    }
}

/// Solve a quadratic equation `expr = 0` for variable `var`.
/// Assumes `expr` is of the form `a*var^2 + b*var + c`.
/// Returns (root1, root2) or None if not quadratic or no real roots.
pub fn solve_quadratic(expr: &Expr, var: &str) -> Option<(f64, f64)> {
    let (a, b, c) = extract_quadratic_coeffs(expr, var)?;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sqrt_disc = disc.sqrt();
    let denom = 2.0 * a;
    if denom.abs() < 1e-15 {
        return None;
    }
    Some(((-b - sqrt_disc) / denom, (-b + sqrt_disc) / denom))
}

fn extract_quadratic_coeffs(expr: &Expr, var: &str) -> Option<(f64, f64, f64)> {
    match expr {
        Expr::Const(c) => Some((0.0, 0.0, *c)),
        Expr::Var(v) if v == var => Some((0.0, 1.0, 0.0)),
        Expr::Var(_) => Some((0.0, 0.0, 0.0)),
        Expr::Add(a, b) => {
            let (a1, b1, c1) = extract_quadratic_coeffs(a, var)?;
            let (a2, b2, c2) = extract_quadratic_coeffs(b, var)?;
            Some((a1 + a2, b1 + b2, c1 + c2))
        }
        Expr::Sub(a, b) => {
            let (a1, b1, c1) = extract_quadratic_coeffs(a, var)?;
            let (a2, b2, c2) = extract_quadratic_coeffs(b, var)?;
            Some((a1 - a2, b1 - b2, c1 - c2))
        }
        Expr::Mul(a, b) => {
            // Try: const * var, var * var, const * var^2
            match (a.as_ref(), b.as_ref()) {
                (&Expr::Const(coeff), Expr::Var(v)) if v == var => Some((0.0, coeff, 0.0)),
                (Expr::Var(v), &Expr::Const(coeff)) if v == var => Some((0.0, coeff, 0.0)),
                (Expr::Var(v1), Expr::Var(v2)) if v1 == var && v2 == var => Some((1.0, 0.0, 0.0)),
                (&Expr::Const(coeff), inner) => {
                    let (a1, b1, c1) = extract_quadratic_coeffs(inner, var)?;
                    Some((a1 * coeff, b1 * coeff, c1 * coeff))
                }
                (inner, &Expr::Const(coeff)) => {
                    let (a1, b1, c1) = extract_quadratic_coeffs(inner, var)?;
                    Some((a1 * coeff, b1 * coeff, c1 * coeff))
                }
                _ => None,
            }
        }
        Expr::Pow(base, exp) => {
            if let &Expr::Const(2.0) = exp.as_ref() {
                if let Expr::Var(v) = base.as_ref() {
                    if v == var {
                        return Some((1.0, 0.0, 0.0));
                    }
                }
            }
            None
        }
        Expr::Neg(a) => {
            let (a1, b1, c1) = extract_quadratic_coeffs(a, var)?;
            Some((-a1, -b1, -c1))
        }
        _ => None,
    }
}

/// A simple gradient-descent optimizer.
#[derive(Debug, Clone)]
pub struct Optimizer {
    pub lr: f64,
    pub momentum: f64,
    pub iterations: usize,
}

impl Default for Optimizer {
    fn default() -> Self {
        Self {
            lr: 0.01,
            momentum: 0.9,
            iterations: 1000,
        }
    }
}

impl Optimizer {
    pub fn new(lr: f64) -> Self {
        Self {
            lr,
            ..Default::default()
        }
    }

    pub fn with_iterations(mut self, iters: usize) -> Self {
        self.iterations = iters;
        self
    }

    /// Minimize a function `f` starting from `x0`, returning (argmin, fmin).
    pub fn minimize<F: Fn(f64) -> f64>(&self, f: F, mut x: f64) -> (f64, f64) {
        let mut velocity = 0.0;
        let eps = 1e-6;
        for _ in 0..self.iterations {
            let grad = (f(x + eps) - f(x - eps)) / (2.0 * eps);
            velocity = self.momentum * velocity - self.lr * grad;
            x += velocity;
        }
        (x, f(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbolic::parse;

    #[test]
    fn solve_linear_simple() {
        // 2*x - 4 = 0 → x = 2
        let e = parse("2 * x - 4").unwrap();
        assert!((solve_linear(&e, "x").unwrap() - 2.0).abs() < 1e-8);
    }

    #[test]
    fn solve_quadratic_simple() {
        // x^2 - 4 = 0 → x = ±2
        let e = parse("x ^ 2 - 4").unwrap();
        let (r1, r2) = solve_quadratic(&e, "x").unwrap();
        assert!(
            ((r1 + 2.0).abs() < 1e-8 && (r2 - 2.0).abs() < 1e-8)
                || ((r1 - 2.0).abs() < 1e-8 && (r2 + 2.0).abs() < 1e-8)
        );
    }

    #[test]
    fn optimizer_minimizes_quadratic() {
        let opt = Optimizer::new(0.1).with_iterations(500);
        let (x, y) = opt.minimize(|x| (x - 3.0).powi(2), 0.0);
        assert!((x - 3.0).abs() < 1e-3);
        assert!(y.abs() < 1e-5);
    }
}
