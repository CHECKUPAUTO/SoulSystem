use std::ops::{Add, Sub, Mul, Div, Neg};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dual {
    pub primal: f64,
    pub tangent: f64,
}

impl Dual {
    pub fn new(primal: f64, tangent: f64) -> Self {
        Self { primal, tangent }
    }
    pub fn primal(c: f64) -> Self {
        Self { primal: c, tangent: 0.0 }
    }
    pub fn var(v: f64) -> Self {
        Self { primal: v, tangent: 1.0 }
    }
    pub fn val(&self) -> f64 { self.primal }
    pub fn grad(&self) -> f64 { self.tangent }

    pub fn sin(self) -> Self {
        Self { primal: self.primal.sin(), tangent: self.primal.cos() * self.tangent }
    }
    pub fn cos(self) -> Self {
        Self { primal: self.primal.cos(), tangent: -self.primal.sin() * self.tangent }
    }
    pub fn exp(self) -> Self {
        let e = self.primal.exp();
        Self { primal: e, tangent: e * self.tangent }
    }
    pub fn ln(self) -> Self {
        Self { primal: self.primal.ln(), tangent: self.tangent / self.primal }
    }
    pub fn sqrt(self) -> Self {
        let s = self.primal.sqrt();
        Self { primal: s, tangent: self.tangent / (2.0 * s) }
    }
    pub fn powi(self, n: i32) -> Self {
        Self { primal: self.primal.powi(n), tangent: (n as f64) * self.primal.powi(n-1) * self.tangent }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { primal: self.primal + rhs.primal, tangent: self.tangent + rhs.tangent }
    }
}

impl Sub for Dual {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self { primal: self.primal - rhs.primal, tangent: self.tangent - rhs.tangent }
    }
}

impl Mul for Dual {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self { primal: self.primal * rhs.primal, tangent: self.primal * rhs.tangent + self.tangent * rhs.primal }
    }
}

impl Div for Dual {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        Self { primal: self.primal / rhs.primal, tangent: (self.tangent * rhs.primal - self.primal * rhs.tangent) / (rhs.primal * rhs.primal) }
    }
}

impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        Self { primal: -self.primal, tangent: -self.tangent }
    }
}
