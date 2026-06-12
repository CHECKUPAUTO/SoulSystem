use scirust_symbolic::Expr;

// ---------------------------------------------------------------------------
// Equation solver
// ---------------------------------------------------------------------------

/// Solve linear equation ax + b = 0 → x = -b/a
pub fn solve_linear(expr: &Expr, var: &str) -> Option<f64> {
    match expr {
        Expr::Add(left, right) => {
            match (&**left, &**right) {
                (Expr::Mul(a_box, b_box), Expr::Const(c)) |
                (Expr::Const(c), Expr::Mul(a_box, b_box)) => {
                    match (&**a_box, &**b_box) {
                        (Expr::Const(a), Expr::Var(v)) |
                        (Expr::Var(v), Expr::Const(a)) if v == var => {
                            if *a != 0.0 {
                                return Some(-c / a);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Expr::Sub(left, right) => {
            match (&**left, &**right) {
                (Expr::Mul(a_box, b_box), Expr::Const(c)) => {
                    match (&**a_box, &**b_box) {
                        (Expr::Const(a), Expr::Var(v)) |
                        (Expr::Var(v), Expr::Const(a)) if v == var => {
                            if *a != 0.0 {
                                return Some(c / a);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    None
}

/// Solve quadratic equation ax² + bx + c = 0 using the quadratic formula.
/// Returns up to two real roots.
pub fn solve_quadratic(expr: &Expr, var: &str) -> Vec<f64> {
    let mut roots = Vec::new();
    // Try to match Add(Pow(Var(x), Const(2.0)), Add(Mul(Const(b), Var(x)), Const(c)))
    // or flattened forms. For simplicity, collect coefficients.
    if let Some((a, b, c)) = extract_quadratic_coeffs(expr, var) {
        if a == 0.0 {
            // Degenerate to linear
            if b != 0.0 {
                roots.push(-c / b);
            }
            return roots;
        }
        let discriminant = b * b - 4.0 * a * c;
        if discriminant >= 0.0 {
            let sqrt_d = discriminant.sqrt();
            roots.push((-b + sqrt_d) / (2.0 * a));
            roots.push((-b - sqrt_d) / (2.0 * a));
        }
    }
    roots
}

/// Extract coefficients (a, b, c) for ax² + bx + c from an expression.
fn extract_quadratic_coeffs(expr: &Expr, var: &str) -> Option<(f64, f64, f64)> {
    let mut a = 0.0;
    let mut b_coeff = 0.0;
    let mut c = 0.0;

    // We handle a flattened sum of terms
    let terms = flatten_add(expr);
    for term in terms {
        match classify_term(&term, var) {
            TermKind::Quad(coef) => a += coef,
            TermKind::Linear(coef) => b_coeff += coef,
            TermKind::Const(coef) => c += coef,
            TermKind::Other => return None,
        }
    }
    Some((a, b_coeff, c))
}

#[derive(Debug, Clone, Copy)]
enum TermKind {
    Quad(f64),
    Linear(f64),
    Const(f64),
    Other,
}

fn flatten_add(expr: &Expr) -> Vec<Expr> {
    let mut terms = Vec::new();
    flatten_add_rec(expr, &mut terms);
    terms
}

fn flatten_add_rec(expr: &Expr, terms: &mut Vec<Expr>) {
    if let Expr::Add(left, right) = expr {
        flatten_add_rec(left, terms);
        flatten_add_rec(right, terms);
    } else if let Expr::Sub(left, right) = expr {
        flatten_add_rec(left, terms);
        terms.push(Expr::Neg(Box::new((**right).clone())));
    } else {
        terms.push(expr.clone());
    }
}

fn classify_term(expr: &Expr, var: &str) -> TermKind {
    match expr {
        Expr::Const(v) => TermKind::Const(*v),
        Expr::Var(name) if name == var => TermKind::Linear(1.0),
        Expr::Mul(left, right) => {
            match (&**left, &**right) {
                (Expr::Const(c), Expr::Var(name)) if name == var => TermKind::Linear(*c),
                (Expr::Var(name), Expr::Const(c)) if name == var => TermKind::Linear(*c),
                (Expr::Const(c), Expr::Pow(b, e)) => {
                    if let (Expr::Var(name), Expr::Const(exp)) = (&**b, &**e) {
                        if name == var && (*exp - 2.0).abs() < 1e-12 {
                            return TermKind::Quad(*c);
                        }
                    }
                    TermKind::Other
                }
                (Expr::Pow(b, e), Expr::Const(c)) => {
                    if let (Expr::Var(name), Expr::Const(exp)) = (&**b, &**e) {
                        if name == var && (*exp - 2.0).abs() < 1e-12 {
                            return TermKind::Quad(*c);
                        }
                    }
                    TermKind::Other
                }
                _ => TermKind::Other,
            }
        }
        Expr::Pow(base, exp) => {
            if let (Expr::Var(name), Expr::Const(e)) = (&**base, &**exp) {
                if name == var && (*e - 2.0).abs() < 1e-12 {
                    return TermKind::Quad(1.0);
                }
            }
            TermKind::Other
        }
        Expr::Neg(inner) => {
            match classify_term(inner, var) {
                TermKind::Const(c) => TermKind::Const(-c),
                TermKind::Linear(c) => TermKind::Linear(-c),
                TermKind::Quad(c) => TermKind::Quad(-c),
                TermKind::Other => TermKind::Other,
            }
        }
        _ => TermKind::Other,
    }
}

// ---------------------------------------------------------------------------
// Optimizer: gradient descent + Newton's method using Dual numbers
// ---------------------------------------------------------------------------

use scirust_autodiff::Dual;

pub struct Optimizer {
    pub lr: f64,
    pub max_iter: usize,
    pub tol: f64,
}

impl Optimizer {
    pub fn new(lr: f64, max_iter: usize) -> Self {
        Self { lr, max_iter, tol: 1e-10 }
    }

    pub fn with_tolerance(mut self, tol: f64) -> Self {
        self.tol = tol;
        self
    }

    /// Minimize a 1D function f(x) using gradient descent.
    pub fn minimize_1d<F>(&self, f: F, mut x: f64) -> f64
    where
        F: Fn(Dual) -> Dual,
    {
        for _ in 0..self.max_iter {
            let grad = scirust_autodiff::derivative_1d(&f, x);
            if grad.abs() < self.tol {
                break;
            }
            x -= self.lr * grad;
        }
        x
    }

    /// Find a root of f(x) = 0 using Newton's method with Dual numbers.
    pub fn newton_1d<F>(&self, f: F, mut x: f64) -> Option<f64>
    where
        F: Fn(Dual) -> Dual,
    {
        for _ in 0..self.max_iter {
            let fx = f(Dual::primal(x)).value;
            if fx.abs() < self.tol {
                return Some(x);
            }
            let dfx = scirust_autodiff::derivative_1d(&f, x);
            if dfx.abs() < 1e-14 {
                return None; // derivative too small
            }
            x -= fx / dfx;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Transformation rules
// ---------------------------------------------------------------------------

pub fn apply_trig_identity(expr: &Expr) -> Expr {
    match expr {
        // sin^2(x) + cos^2(x) → 1
        Expr::Add(a, b) => {
            let ra = apply_trig_identity(a);
            let rb = apply_trig_identity(b);
            match (&ra, &rb) {
                (Expr::Pow(sin_box, pow1), Expr::Pow(cos_box, pow2)) => {
                    if let (Expr::Const(2.0), Expr::Const(2.0)) = (&**pow1, &**pow2) {
                        if let (Expr::Sin(arg1), Expr::Cos(arg2)) = (&**sin_box, &**cos_box) {
                            if arg1 == arg2 {
                                return Expr::Const(1.0);
                            }
                        }
                    }
                }
                _ => {}
            }
            Expr::Add(Box::new(ra), Box::new(rb))
        }
        // Default: recurse
        Expr::Mul(a, b) => Expr::Mul(Box::new(apply_trig_identity(a)), Box::new(apply_trig_identity(b))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(apply_trig_identity(a)), Box::new(apply_trig_identity(b))),
        Expr::Div(a, b) => Expr::Div(Box::new(apply_trig_identity(a)), Box::new(apply_trig_identity(b))),
        Expr::Pow(a, b) => Expr::Pow(Box::new(apply_trig_identity(a)), Box::new(apply_trig_identity(b))),
        Expr::Neg(a) => Expr::Neg(Box::new(apply_trig_identity(a))),
        Expr::Sin(a) => Expr::Sin(Box::new(apply_trig_identity(a))),
        Expr::Cos(a) => Expr::Cos(Box::new(apply_trig_identity(a))),
        Expr::Exp(a) => Expr::Exp(Box::new(apply_trig_identity(a))),
        Expr::Ln(a) => Expr::Ln(Box::new(apply_trig_identity(a))),
        Expr::Sqrt(a) => Expr::Sqrt(Box::new(apply_trig_identity(a))),
        Expr::Abs(a) => Expr::Abs(Box::new(apply_trig_identity(a))),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Proof engine: symbolic equality checker
// ---------------------------------------------------------------------------

/// Check if two expressions are provably equal via simplification.
/// Returns true if `simplify(a - b) == 0`.
pub fn prove_equal(a: &Expr, b: &Expr) -> bool {
    let diff = Expr::Sub(Box::new(a.clone()), Box::new(b.clone()));
    let simplified = scirust_symbolic::simplify(&diff);
    match simplified {
        Expr::Const(c) => c.abs() < 1e-12,
        _ => false,
    }
}

/// Check if expression is structurally a known identity.
pub fn is_known_identity(expr: &Expr) -> bool {
    match expr {
        // x + 0 = x, x * 1 = x, x * 0 = 0
        Expr::Add(_, b) => {
            if let Expr::Const(0.0) = &**b { true } else { false }
        }
        Expr::Mul(_, b) => {
            if let Expr::Const(1.0) = &**b { true }
            else if let Expr::Const(0.0) = &**b { true }
            else { false }
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
    fn test_solve_quadratic() {
        // x^2 - 5x + 6 = 0 → roots 2 and 3
        let e = parse("x^2 - 5*x + 6").unwrap();
        let mut roots = solve_quadratic(&e, "x");
        roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(roots, vec![2.0, 3.0]);
    }

    #[test]
    fn test_newton() {
        // f(x) = x^2 - 4, root = 2
        let opt = Optimizer::new(0.1, 50);
        let root = opt.newton_1d(|x| x * x - Dual::primal(4.0), 3.0);
        assert!(root.is_some());
        assert!((root.unwrap() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_prove_equal() {
        // x + 0 = x → simplify(x + 0 - x) = 0
        let a = parse("x + 0").unwrap();
        let b = parse("x").unwrap();
        assert!(prove_equal(&a, &b));

        // (x + 1) - 1 = x → simplify((x + 1) - 1 - x) = 0
        let c = parse("(x + 1) - 1").unwrap();
        let d = parse("x").unwrap();
        assert!(prove_equal(&c, &d));
    }
}
