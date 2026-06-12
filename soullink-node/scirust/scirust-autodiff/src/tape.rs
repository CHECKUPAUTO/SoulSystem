//! Reverse-mode automatic differentiation (backpropagation).
//!
//! Uses a Wengert list (tape) to record operations during the forward pass,
//! then traverses backward to accumulate gradients.
//!
//! # Example
//!
//! ```
//! use scirust_autodiff::tape::{Tape, Var};
//!
//! let tape = Tape::new();
//! let x = tape.var("x", 2.0);
//! let y = tape.var("y", 3.0);
//! let z = x.mul(&y).add(&x.sin());
//! tape.backward(&z);
//!
//! assert!((x.grad() - (3.0 + 2.0f64.cos())).abs() < 1e-10);
//! assert!((y.grad() - 2.0).abs() < 1e-10);
//! ```

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Op enum
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Op {
    Input, Constant,
    Add, Sub, Mul, Div, Neg,
    Sin, Cos, Exp, Ln,
    Pow(f64),
}

// ---------------------------------------------------------------------------
// Shared tape storage
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TapeNode {
    op: Op,
    in_a: usize,
    in_b: usize,
    num_in: usize,
    value: f64,
    grad: f64,
}

/// Internal tape storage shared between Var instances.
#[derive(Clone)]
pub struct TapeInner {
    nodes: Vec<TapeNode>,
}

impl TapeInner {
    fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    fn alloc(&mut self, op: Op, in_a: usize, in_b: usize, num_in: usize, value: f64) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(TapeNode { op, in_a, in_b, num_in, value, grad: 0.0 });
        idx
    }
}

// ---------------------------------------------------------------------------
// Tape
// ---------------------------------------------------------------------------

/// Records operations for reverse-mode automatic differentiation.
pub struct Tape {
    inner: Rc<RefCell<TapeInner>>,
}

impl Tape {
    pub fn new() -> Self {
        Self { inner: Rc::new(RefCell::new(TapeInner::new())) }
    }

    /// Register a new input variable.
    pub fn var(&self, name: &str, value: f64) -> Var {
        let idx = self.inner.borrow_mut().alloc(Op::Input, 0, 0, 0, value);
        Var { idx, inner: self.inner.clone(), _name: name.to_string() }
    }

    /// Run backward pass from the given output variable.
    pub fn backward(&self, output: &Var) {
        let mut inner = self.inner.borrow_mut();
        let n = inner.nodes.len();
        if output.idx >= n { return; }

        inner.nodes[output.idx].grad = 1.0;

        for i in (0..=output.idx).rev() {
            // Extract all data we need before mutating
            let g = inner.nodes[i].grad;
            let op = inner.nodes[i].op;
            let in_a = inner.nodes[i].in_a;
            let in_b = inner.nodes[i].in_b;
            let num_in = inner.nodes[i].num_in;

            // Extract values we'll need
            let (val_a, val_b, val_self) = if num_in > 0 {
                let va = if in_a < n { inner.nodes[in_a].value } else { 0.0 };
                let vb = if in_b < n && in_b != in_a { inner.nodes[in_b].value } else { va };
                let vs = inner.nodes[i].value;
                (va, vb, vs)
            } else {
                (0.0, 0.0, 0.0)
            };

            match op {
                Op::Add => {
                    if num_in >= 1 && in_a < n { inner.nodes[in_a].grad += g; }
                    if num_in >= 2 && in_b < n && in_b != in_a { inner.nodes[in_b].grad += g; }
                }
                Op::Sub => {
                    if num_in >= 1 && in_a < n { inner.nodes[in_a].grad += g; }
                    if num_in >= 2 && in_b < n && in_b != in_a { inner.nodes[in_b].grad -= g; }
                }
                Op::Mul => {
                    if num_in >= 2 {
                        if in_a < n { inner.nodes[in_a].grad += g * val_b; }
                        if in_b < n { inner.nodes[in_b].grad += g * val_a; }
                    }
                }
                Op::Div => {
                    if num_in >= 2 {
                        let y = val_b;
                        if in_a < n { inner.nodes[in_a].grad += g / y; }
                        if in_b < n && in_b != in_a { inner.nodes[in_b].grad -= g * val_a / (y * y); }
                    }
                }
                Op::Neg => {
                    if in_a < n { inner.nodes[in_a].grad -= g; }
                }
                Op::Sin => {
                    if in_a < n { inner.nodes[in_a].grad += g * val_a.cos(); }
                }
                Op::Cos => {
                    if in_a < n { inner.nodes[in_a].grad -= g * val_a.sin(); }
                }
                Op::Exp => {
                    if in_a < n { inner.nodes[in_a].grad += g * val_self; }
                }
                Op::Ln => {
                    if in_a < n { inner.nodes[in_a].grad += g / val_a; }
                }
                Op::Pow(pow_n) => {
                    if in_a < n { inner.nodes[in_a].grad += g * pow_n * val_a.powf(pow_n - 1.0); }
                }
                _ => {}
            }
        }
    }

    /// Reset all gradients to zero.
    pub fn zero_grad(&self) {
        for node in &mut self.inner.borrow_mut().nodes {
            node.grad = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------
// Var
// ---------------------------------------------------------------------------

/// A differentiable variable tracked by the tape.
#[derive(Clone)]
pub struct Var {
    idx: usize,
    inner: Rc<RefCell<TapeInner>>,
    _name: String,
}

impl Var {
    fn alloc(&self, op: Op, in_a: usize, in_b: usize, num_in: usize, value: f64) -> Var {
        let idx = self.inner.borrow_mut().alloc(op, in_a, in_b, num_in, value);
        Var { idx, inner: self.inner.clone(), _name: String::new() }
    }

    /// Get the current value.
    pub fn val(&self) -> f64 {
        self.inner.borrow().nodes[self.idx].value
    }

    /// Get the gradient (call after `tape.backward()`).
    pub fn grad(&self) -> f64 {
        self.inner.borrow().nodes[self.idx].grad
    }

    // --- Arithmetic (method-based to avoid trait resolution issues) ---

    pub fn add(&self, other: &Var) -> Var {
        self.alloc(Op::Add, self.idx, other.idx, 2, self.val() + other.val())
    }

    pub fn sub(&self, other: &Var) -> Var {
        self.alloc(Op::Sub, self.idx, other.idx, 2, self.val() - other.val())
    }

    pub fn mul(&self, other: &Var) -> Var {
        self.alloc(Op::Mul, self.idx, other.idx, 2, self.val() * other.val())
    }

    pub fn div(&self, other: &Var) -> Var {
        self.alloc(Op::Div, self.idx, other.idx, 2, self.val() / other.val())
    }

    pub fn neg(&self) -> Var {
        self.alloc(Op::Neg, self.idx, 0, 1, -self.val())
    }

    pub fn add_scalar(&self, s: f64) -> Var {
        let c = self.alloc(Op::Constant, 0, 0, 0, s);
        self.alloc(Op::Add, self.idx, c.idx, 2, self.val() + s)
    }

    pub fn mul_scalar(&self, s: f64) -> Var {
        let c = self.alloc(Op::Constant, 0, 0, 0, s);
        self.alloc(Op::Mul, self.idx, c.idx, 2, self.val() * s)
    }

    // --- Math functions ---

    pub fn sin(&self) -> Var {
        self.alloc(Op::Sin, self.idx, 0, 1, self.val().sin())
    }

    pub fn cos(&self) -> Var {
        self.alloc(Op::Cos, self.idx, 0, 1, self.val().cos())
    }

    pub fn exp(&self) -> Var {
        self.alloc(Op::Exp, self.idx, 0, 1, self.val().exp())
    }

    pub fn ln(&self) -> Var {
        self.alloc(Op::Ln, self.idx, 0, 1, self.val().ln())
    }

    pub fn powi(&self, n: i32) -> Var {
        self.alloc(Op::Pow(n as f64), self.idx, 0, 1, self.val().powi(n))
    }

    pub fn sqrt(&self) -> Var {
        self.alloc(Op::Pow(0.5), self.idx, 0, 1, self.val().sqrt())
    }
}

impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Var(val={}, grad={})", self.val(), self.grad())
    }
}

impl fmt::Display for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Var(val={})", self.val())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let tape = Tape::new();
        let a = tape.var("a", 2.0);
        let b = tape.var("b", 3.0);
        let c = a.add(&b);
        tape.backward(&c);
        assert!((a.grad() - 1.0).abs() < 1e-10);
        assert!((b.grad() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_mul() {
        let tape = Tape::new();
        let a = tape.var("a", 2.0);
        let b = tape.var("b", 3.0);
        let c = a.mul(&b);
        tape.backward(&c);
        assert!((a.grad() - 3.0).abs() < 1e-10);
        assert!((b.grad() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_chain_rule() {
        let tape = Tape::new();
        let x = tape.var("x", 1.0);
        let y = x.mul(&x);  // y = x²
        let z = y.add(&x);  // z = x² + x
        tape.backward(&z);
        assert!((x.grad() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_sin() {
        let tape = Tape::new();
        let x = tape.var("x", std::f64::consts::PI / 2.0);
        let y = x.sin();
        tape.backward(&y);
        assert!((x.grad() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_powi() {
        let tape = Tape::new();
        let x = tape.var("x", 3.0);
        let y = x.powi(3);
        tape.backward(&y);
        assert!((x.grad() - 27.0).abs() < 1e-10);
    }

    #[test]
    fn test_gradient_descent() {
        let tape = Tape::new();
        let x = tape.var("x", 5.0);
        let two = tape.var("two", 2.0);
        let f = x.sub(&two).powi(2);
        tape.backward(&f);

        let lr = 0.1;
        let new_x = x.val() - lr * x.grad();
        assert!((new_x - 4.4).abs() < 1e-10);
    }

    #[test]
    fn test_multi_use() {
        let tape = Tape::new();
        let x = tape.var("x", 2.0);
        let y = tape.var("y", 3.0);
        let z = x.mul(&y).add(&x.mul(&y.sin()));
        tape.backward(&z);
        assert!(x.grad().is_finite());
        assert!(y.grad().is_finite());
    }

    #[test]
    fn test_scalar_ops() {
        let tape = Tape::new();
        let x = tape.var("x", 3.0);
        let y = x.add_scalar(5.0);
        tape.backward(&y);
        assert!((x.grad() - 1.0).abs() < 1e-10);
        assert!((y.val() - 8.0).abs() < 1e-10);
    }

    #[test]
    fn test_complex_expr() {
        let tape = Tape::new();
        let x = tape.var("x", 2.0);
        let f = x.powi(3).add(&x.sin().mul(&x.exp()));
        tape.backward(&f);
        let expected = 3.0 * 4.0 + 2.0f64.cos() * 2.0f64.exp() + 2.0f64.sin() * 2.0f64.exp();
        assert!((x.grad() - expected).abs() < 1e-8, "got {} expected {}", x.grad(), expected);
    }

    #[test]
    fn test_sub() {
        let tape = Tape::new();
        let a = tape.var("a", 10.0);
        let b = tape.var("b", 3.0);
        let c = a.sub(&b);
        tape.backward(&c);
        assert!((a.grad() - 1.0).abs() < 1e-10);
        assert!((b.grad() + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_mul_scalar() {
        let tape = Tape::new();
        let x = tape.var("x", 7.0);
        let y = x.mul_scalar(3.0);
        tape.backward(&y);
        assert!((x.grad() - 3.0).abs() < 1e-10);
        assert!((y.val() - 21.0).abs() < 1e-10);
    }
}
