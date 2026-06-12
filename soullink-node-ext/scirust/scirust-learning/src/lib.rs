use scirust_symbolic::Expr;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Pattern Memory with scoring and aging
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct PatternMemory {
    hits: HashMap<String, usize>,
    transformations: HashMap<String, Expr>,
    scores: HashMap<String, f64>, // higher = more useful
}

impl PatternMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, input: &Expr, output: &Expr, success: bool) {
        let key = format!("{}", input);
        *self.hits.entry(key.clone()).or_insert(0) += 1;
        self.transformations.insert(key.clone(), output.clone());
        // Boost score on success, decay slightly on failure
        let delta = if success { 1.0 } else { -0.5 };
        let entry = self.scores.entry(key).or_insert(0.0);
        *entry = (*entry + delta).max(0.0);
    }

    pub fn recall(&self, input: &Expr) -> Option<&Expr> {
        let key = format!("{}", input);
        self.transformations.get(&key)
    }

    pub fn most_used(&self, n: usize) -> Vec<(&String, usize)> {
        let mut v: Vec<_> = self.hits.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        v.into_iter().take(n).map(|(k, v)| (k, *v)).collect()
    }

    pub fn best_patterns(&self, n: usize) -> Vec<(&String, f64)> {
        let mut v: Vec<_> = self.scores.iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        v.into_iter().take(n).map(|(k, v)| (k, *v)).collect()
    }

    pub fn prune_low_scoring(&mut self, threshold: f64) {
        let to_remove: Vec<String> = self
            .scores
            .iter()
            .filter(|(_, s)| **s < threshold)
            .map(|(k, _)| k.clone())
            .collect();
        for k in to_remove {
            self.hits.remove(&k);
            self.transformations.remove(&k);
            self.scores.remove(&k);
        }
    }
}

// ---------------------------------------------------------------------------
// Auto Feature Discovery: detect algebraic patterns in expressions
// ---------------------------------------------------------------------------

/// A discovered pattern in a symbolic expression.
#[derive(Debug, Clone)]
pub struct DiscoveredPattern {
    pub description: String,
    pub confidence: f64, // 0.0 to 1.0
}

/// Scan an expression tree for common algebraic patterns.
pub fn discover_patterns(expr: &Expr) -> Vec<DiscoveredPattern> {
    let mut patterns = Vec::new();
    discover_patterns_rec(expr, &mut patterns);
    patterns
}

fn discover_patterns_rec(expr: &Expr, patterns: &mut Vec<DiscoveredPattern>) {
    match expr {
        Expr::Add(a, b) => {
            // Check for even / odd structure
            if is_even_function(a) && is_even_function(b) {
                patterns.push(DiscoveredPattern {
                    description: "sum of even functions → even".to_string(),
                    confidence: 0.9,
                });
            }
            // Check for telescoping potential
            if is_negative_of(a, b) {
                patterns.push(DiscoveredPattern {
                    description: "telescoping pair: f + (-f)".to_string(),
                    confidence: 1.0,
                });
            }
            discover_patterns_rec(a, patterns);
            discover_patterns_rec(b, patterns);
        }
        Expr::Mul(a, b) => {
            if is_constant(a) && is_variable(b) {
                patterns.push(DiscoveredPattern {
                    description: format!("linear term: {} * {}", a, b),
                    confidence: 0.95,
                });
            }
            if is_variable(a) && is_variable(b) && a == b {
                patterns.push(DiscoveredPattern {
                    description: "quadratic term: variable squared".to_string(),
                    confidence: 0.95,
                });
            }
            discover_patterns_rec(a, patterns);
            discover_patterns_rec(b, patterns);
        }
        Expr::Pow(base, exp) => {
            if is_variable(base) && is_constant(exp) {
                if let Expr::Const(n) = &**exp {
                    patterns.push(DiscoveredPattern {
                        description: format!("polynomial degree {:.0}", n),
                        confidence: 0.95,
                    });
                }
            }
            discover_patterns_rec(base, patterns);
            discover_patterns_rec(exp, patterns);
        }
        Expr::Sin(a) | Expr::Cos(a) | Expr::Exp(a) | Expr::Ln(a) | Expr::Sqrt(a) | Expr::Abs(a) | Expr::Neg(a) => {
            discover_patterns_rec(a, patterns);
        }
        Expr::Sub(a, b) => {
            discover_patterns_rec(a, patterns);
            discover_patterns_rec(b, patterns);
        }
        Expr::Div(a, b) => {
            if is_constant(a) && is_constant(b) {
                patterns.push(DiscoveredPattern {
                    description: "constant ratio".to_string(),
                    confidence: 0.9,
                });
            }
            discover_patterns_rec(a, patterns);
            discover_patterns_rec(b, patterns);
        }
        Expr::Const(_) | Expr::Var(_) => {}
    }
}

fn is_even_function(expr: &Expr) -> bool {
    matches!(expr, Expr::Cos(_) | Expr::Abs(_))
}

fn is_negative_of(a: &Expr, b: &Expr) -> bool {
    if let Expr::Neg(inner) = a {
        return **inner == *b;
    }
    if let Expr::Neg(inner) = b {
        return **inner == *a;
    }
    false
}

fn is_constant(expr: &Expr) -> bool {
    matches!(expr, Expr::Const(_))
}

fn is_variable(expr: &Expr) -> bool {
    matches!(expr, Expr::Var(_))
}

// ---------------------------------------------------------------------------
// Function Approximation: polynomial least-squares fit
// ---------------------------------------------------------------------------

/// Fit a polynomial of degree `deg` to data points (x, y).
/// Returns coefficients [c0, c1, ..., c_deg] for c0 + c1*x + ...
pub fn polynomial_fit(xs: &[f64], ys: &[f64], deg: usize) -> Result<Vec<f64>, String> {
    if xs.len() != ys.len() {
        return Err("xs and ys must have same length".to_string());
    }
    if xs.len() <= deg {
        return Err("Need more points than degree".to_string());
    }

    let n = xs.len();
    let m = deg + 1;

    // Build Vandermonde matrix A (n x m) and vector b (n)
    let mut ata = vec![vec![0.0; m]; m];
    let mut atb = vec![0.0; m];

    for i in 0..n {
        let xi = xs[i];
        let mut row = vec![1.0; m];
        for j in 1..m {
            row[j] = row[j - 1] * xi;
        }
        for j in 0..m {
            atb[j] += row[j] * ys[i];
            for k in 0..m {
                ata[j][k] += row[j] * row[k];
            }
        }
    }

    // Gaussian elimination for symmetric positive-definite system
    let mut a = ata;
    let mut b = atb;

    for col in 0..m {
        // Find pivot
        let mut max_row = col;
        for row in (col + 1)..m {
            if a[row][col].abs() > a[max_row][col].abs() {
                max_row = row;
            }
        }
        if a[max_row][col].abs() < 1e-12 {
            return Err("Singular matrix in polynomial fit".to_string());
        }
        a.swap(col, max_row);
        b.swap(col, max_row);

        // Eliminate below
        for row in (col + 1)..m {
            let factor = a[row][col] / a[col][col];
            for k in col..m {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }

    // Back substitution
    let mut coeffs = vec![0.0; m];
    for i in (0..m).rev() {
        let mut sum = b[i];
        for j in (i + 1)..m {
            sum -= a[i][j] * coeffs[j];
        }
        coeffs[i] = sum / a[i][i];
    }

    Ok(coeffs)
}

// ---------------------------------------------------------------------------
// Regression Core: simple linear regression y = a*x + b
// ---------------------------------------------------------------------------

pub fn linear_regression(xs: &[f64], ys: &[f64]) -> Result<(f64, f64), String> {
    if xs.len() != ys.len() || xs.is_empty() {
        return Err("Invalid data for linear regression".to_string());
    }
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;

    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..xs.len() {
        let dx = xs[i] - mean_x;
        num += dx * (ys[i] - mean_y);
        den += dx * dx;
    }

    if den.abs() < 1e-12 {
        return Err("Zero variance in x".to_string());
    }

    let slope = num / den;
    let intercept = mean_y - slope * mean_x;
    Ok((slope, intercept))
}

// ---------------------------------------------------------------------------
// Tensor helpers (basic ndarray integration)
// ---------------------------------------------------------------------------

pub mod tensor {
    pub use ndarray::{Array1, Array2, ArrayView1, ArrayView2};

    /// Element-wise operation on a 1D array.
    pub fn map_array1(a: &Array1<f64>, f: impl Fn(f64) -> f64) -> Array1<f64> {
        a.mapv(f)
    }

    /// Element-wise binary operation on two 1D arrays.
    pub fn zip_array1(a: &Array1<f64>, b: &Array1<f64>, f: impl Fn(f64, f64) -> f64) -> Array1<f64> {
        ndarray::Zip::from(a)
            .and(b)
            .map_collect(|x, y| f(*x, *y))
    }

    /// Matrix-vector product.
    pub fn mat_vec(a: &Array2<f64>, x: &Array1<f64>) -> Array1<f64> {
        a.dot(x)
    }

    /// Outer product of two vectors.
    pub fn outer(u: &Array1<f64>, v: &Array1<f64>) -> Array2<f64> {
        let n = u.len();
        let m = v.len();
        let mut result = Array2::<f64>::zeros((n, m));
        for i in 0..n {
            for j in 0..m {
                result[[i, j]] = u[i] * v[j];
            }
        }
        result
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
    fn test_pattern_memory_scoring() {
        let mut mem = PatternMemory::new();
        let e = parse("x + 0").unwrap();
        let s = parse("x").unwrap();
        mem.record(&e, &s, true);
        mem.record(&e, &s, true);
        let best = mem.best_patterns(1);
        assert_eq!(best.len(), 1);
        assert!(best[0].1 > 0.0);
    }

    #[test]
    fn test_discover_patterns() {
        let e = parse("2*x^2 + 3*x + 1").unwrap();
        let patterns = discover_patterns(&e);
        assert!(!patterns.is_empty());
        let descs: Vec<String> = patterns.iter().map(|p| p.description.clone()).collect();
        assert!(descs.iter().any(|d| d.contains("polynomial degree")));
        assert!(descs.iter().any(|d| d.contains("linear term")));
    }
}
