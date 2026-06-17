//! Forward-mode automatic differentiation via dual numbers.
//!
//! A `Dual` carries a primal value and its derivative. Arithmetic operations
//! propagate derivatives through the chain rule automatically.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A dual number `a + b·ε` where `a` is the primal value and `b` its derivative.
#[derive(Debug, Clone, Copy)]
pub struct Dual {
    primal: f64,
    grad: f64,
}

impl Dual {
    /// Create a variable with value `x` and derivative 1.0.
    pub fn var(x: f64) -> Self {
        Self {
            primal: x,
            grad: 1.0,
        }
    }

    /// Create a constant with value `c` and derivative 0.0.
    pub fn primal(c: f64) -> Self {
        Self {
            primal: c,
            grad: 0.0,
        }
    }

    /// The primal (value) part.
    #[inline]
    pub fn val(&self) -> f64 {
        self.primal
    }

    /// The derivative (gradient) part.
    #[inline]
    pub fn grad(&self) -> f64 {
        self.grad
    }

    /// Sine.
    pub fn sin(self) -> Self {
        Self {
            primal: self.primal.sin(),
            grad: self.grad * self.primal.cos(),
        }
    }

    /// Cosine.
    pub fn cos(self) -> Self {
        Self {
            primal: self.primal.cos(),
            grad: self.grad * (-self.primal.sin()),
        }
    }

    /// Exponential.
    pub fn exp(self) -> Self {
        let e = self.primal.exp();
        Self {
            primal: e,
            grad: self.grad * e,
        }
    }

    /// Natural logarithm.
    pub fn ln(self) -> Self {
        Self {
            primal: self.primal.ln(),
            grad: self.grad / self.primal,
        }
    }

    /// Power to an integer exponent.
    pub fn powi(self, n: i32) -> Self {
        let p = self.primal.powi(n);
        Self {
            primal: p,
            grad: self.grad * (n as f64) * self.primal.powi(n - 1),
        }
    }

    /// Square root.
    pub fn sqrt(self) -> Self {
        let s = self.primal.sqrt();
        Self {
            primal: s,
            grad: self.grad / (2.0 * s),
        }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            primal: self.primal + rhs.primal,
            grad: self.grad + rhs.grad,
        }
    }
}

impl Sub for Dual {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            primal: self.primal - rhs.primal,
            grad: self.grad - rhs.grad,
        }
    }
}

impl Mul for Dual {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            primal: self.primal * rhs.primal,
            grad: self.grad * rhs.primal + self.primal * rhs.grad,
        }
    }
}

impl Div for Dual {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let denom = rhs.primal * rhs.primal;
        Self {
            primal: self.primal / rhs.primal,
            grad: (self.grad * rhs.primal - self.primal * rhs.grad) / denom,
        }
    }
}

impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            primal: -self.primal,
            grad: -self.grad,
        }
    }
}

/// Compute the first derivative of `f` at point `x` using dual numbers.
pub fn deriv<F>(f: F, x: f64) -> f64
where
    F: Fn(Dual) -> Dual,
{
    f(Dual::var(x)).grad()
}

/// Compute the function value and gradient of `f` at `(x, y)`.
pub fn grad2<F>(f: F, x: f64, y: f64) -> (f64, f64)
where
    F: Fn(Dual, Dual) -> Dual,
{
    let fx = f(Dual::var(x), Dual::primal(y));
    let fy = f(Dual::primal(x), Dual::var(y));
    (fx.grad(), fy.grad())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_arithmetic() {
        let x = Dual::var(2.0);
        let c = Dual::primal(3.0);
        // f = 3x => f' = 3
        let f = c * x;
        assert_eq!(f.val(), 6.0);
        assert_eq!(f.grad(), 3.0);
    }

    #[test]
    fn dual_power() {
        // f = x^3 => f' = 3x^2
        let f = Dual::var(2.0).powi(3);
        assert_eq!(f.val(), 8.0);
        assert_eq!(f.grad(), 12.0);
    }

    #[test]
    fn dual_sin() {
        // f = sin(x) => f' = cos(x)
        let f = Dual::var(0.0).sin();
        assert!((f.val() - 0.0).abs() < 1e-10);
        assert!((f.grad() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn deriv_square() {
        // d/dx x^2 = 2x, at x=3 => 6
        let d = deriv(|x| x * x, 3.0);
        assert!((d - 6.0).abs() < 1e-10);
    }

    #[test]
    fn deriv_exp() {
        let d = deriv(|x| x.exp(), 1.0);
        assert!((d - std::f64::consts::E).abs() < 1e-10);
    }

    #[test]
    fn grad2_rosenbrock() {
        let (dx, dy) = grad2(
            |x, y| {
                let one = Dual::primal(1.0);
                let a = Dual::primal(100.0);
                let t1 = y - x * x;
                let t2 = one - x;
                a * t1 * t1 + t2 * t2
            },
            1.0,
            1.0,
        );
        assert!((dx - 0.0).abs() < 1e-10); // gradient = 0 at (1, 1)
        assert!((dy - 0.0).abs() < 1e-10);
    }
}
