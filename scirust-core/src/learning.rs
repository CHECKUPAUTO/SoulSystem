//! Learning and pattern discovery.
//!
//! Includes polynomial fitting via least-squares and expression pattern mining.

use crate::symbolic::Expr;

/// Fit a polynomial of given degree to (x, y) data using least squares.
/// Returns coefficients [a_0, a_1, ..., a_d] for a_0 + a_1*x + ... + a_d*x^d.
pub fn polynomial_fit(xs: &[f64], ys: &[f64], degree: usize) -> Vec<f64> {
    assert_eq!(xs.len(), ys.len(), "xs and ys must have same length");
    assert!(xs.len() > degree, "not enough data points for given degree");
    let n = xs.len();
    let d = degree + 1;

    // Build Vandermonde-like matrix A: A[i][j] = xs[i]^j
    let mut a = vec![0.0f64; n * d];
    for i in 0..n {
        let mut pow = 1.0f64;
        for j in 0..d {
            a[i * d + j] = pow;
            pow *= xs[i];
        }
    }

    // Compute A^T * A (d × d) and A^T * y (d × 1)
    let mut ata = vec![0.0f64; d * d];
    let mut aty = vec![0.0f64; d];
    for i in 0..n {
        for j in 0..d {
            let a_ij = a[i * d + j];
            aty[j] += a_ij * ys[i];
            for k in 0..d {
                ata[j * d + k] += a_ij * a[i * d + k];
            }
        }
    }

    // Solve ATA * coeffs = ATy via Gaussian elimination
    let mut m = vec![0.0f64; d * (d + 1)];
    for j in 0..d {
        for k in 0..d {
            m[j * (d + 1) + k] = ata[j * d + k];
        }
        m[j * (d + 1) + d] = aty[j];
    }

    for col in 0..d {
        // Find pivot
        let mut pivot_row = col;
        let mut pivot_val = m[col * (d + 1) + col].abs();
        for row in (col + 1)..d {
            let val = m[row * (d + 1) + col].abs();
            if val > pivot_val {
                pivot_val = val;
                pivot_row = row;
            }
        }
        if pivot_val < 1e-15 {
            continue;
        }
        // Swap rows
        if pivot_row != col {
            for k in 0..=d {
                m.swap(col * (d + 1) + k, pivot_row * (d + 1) + k);
            }
        }
        // Eliminate
        let pivot = m[col * (d + 1) + col];
        for row in (col + 1)..d {
            let factor = m[row * (d + 1) + col] / pivot;
            for k in col..=d {
                m[row * (d + 1) + k] -= factor * m[col * (d + 1) + k];
            }
        }
    }

    // Back substitution
    let mut coeffs = vec![0.0f64; d];
    for row in (0..d).rev() {
        let mut sum = m[row * (d + 1) + d];
        for col in (row + 1)..d {
            sum -= m[row * (d + 1) + col] * coeffs[col];
        }
        let div = m[row * (d + 1) + row];
        coeffs[row] = if div.abs() > 1e-15 { sum / div } else { 0.0 };
    }
    coeffs
}

/// Discover common sub-patterns in an expression tree.
/// Returns a vector of (sub_expr, occurrence_count) sorted by occurrence.
pub fn discover_patterns(expr: &Expr) -> Vec<(Expr, usize)> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    count_patterns(expr, &mut counts);
    let mut patterns: Vec<(Expr, usize)> = counts
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(s, c)| (Expr::Var(s), c)) // simplified return
        .collect();
    patterns.sort_by(|a, b| b.1.cmp(&a.1));
    patterns
}

fn count_patterns(expr: &Expr, counts: &mut std::collections::HashMap<String, usize>) {
    match expr {
        Expr::Const(_) | Expr::Var(_) => {}
        _ => {
            *counts.entry(format!("{expr}")).or_insert(0) += 1;
        }
    }
    match expr {
        Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Pow(a, b) => {
            count_patterns(a, counts);
            count_patterns(b, counts);
        }
        Expr::Neg(a)
        | Expr::Sin(a)
        | Expr::Cos(a)
        | Expr::Exp(a)
        | Expr::Ln(a)
        | Expr::Sqrt(a)
        | Expr::Abs(a) => {
            count_patterns(a, counts);
        }
        Expr::Const(_) | Expr::Var(_) => {}
    }
}

/// Ordinary least squares linear regression: y = a + b*x.
/// Returns (a, b).
pub fn linear_regression(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let coeffs = polynomial_fit(xs, ys, 1);
    (coeffs[0], coeffs[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polynomial_fit_line() {
        // y = 2 + 3x exact
        let xs = vec![0.0, 1.0, 2.0, 3.0];
        let ys = vec![2.0, 5.0, 8.0, 11.0];
        let coeffs = polynomial_fit(&xs, &ys, 1);
        assert!((coeffs[0] - 2.0).abs() < 1e-10);
        assert!((coeffs[1] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn polynomial_fit_parabola() {
        let xs = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let ys: Vec<f64> = xs.iter().map(|x| x * x).collect();
        let coeffs = polynomial_fit(&xs, &ys, 2);
        assert!((coeffs[0]).abs() < 1e-8); // intercept 0
        assert!((coeffs[1]).abs() < 1e-8); // linear term 0
        assert!((coeffs[2] - 1.0).abs() < 1e-8); // quadratic term 1
    }

    #[test]
    fn linear_regression_works() {
        let xs = vec![1.0, 2.0, 3.0];
        let ys = vec![2.0, 4.0, 6.0];
        let (a, b) = linear_regression(&xs, &ys);
        assert!((a).abs() < 1e-10);
        assert!((b - 2.0).abs() < 1e-10);
    }
}
