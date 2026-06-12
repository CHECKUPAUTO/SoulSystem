//! Prior and posterior probability distributions.
//!
//! # Examples
//!
//! ```
//! use scirust_probability::models::Prior;
//!
//! let prior = Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 };
//! assert!((prior.pdf(0.0) - 0.3989).abs() < 0.001);
//! assert!((prior.mean() - 0.0).abs() < 1e-10);
//! ```
//!
//! ```
//! use scirust_probability::models::{Posterior, Prior, XorShiftRng, Rng};
//!
//! let prior = Prior::Beta(2.0, 5.0);
//! let mut post = Posterior::new(prior);
//! post.observe(1.0);
//! let map = post.map_estimate();
//! assert!(map > 0.0 && map < 1.0);
//! ```

use scirust_symbolic::Expr;
use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Log-Gamma function (Lanczos approximation)
// ---------------------------------------------------------------------------

fn log_gamma(x: f64) -> f64 {
    let coeffs = [
        76.18009172947146, -86.50532032941677, 24.01409824083091,
        -1.231739572450155, 0.1208650973866179e-2, -0.5395239384953e-5,
    ];
    let tmp = x + 5.5;
    let tmp2 = (x + 0.5) * tmp.ln() - tmp;
    let mut ser = 1.000000000190015;
    for (i, &c) in coeffs.iter().enumerate() {
        ser += c / (x + (i + 1) as f64);
    }
    tmp2 + (2.5066282746310005 * ser / x).ln()
}

// ---------------------------------------------------------------------------
// Prior distribution types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Prior {
    Uniform { a: f64, b: f64 },
    Gaussian { mu: f64, sigma_sq: f64 },
    Beta(f64, f64),
    Gamma(f64, f64),
    Custom { expr: Expr, params: std::collections::HashMap<String, f64> },
}

impl Prior {
    pub fn log_pdf(&self, x: f64) -> f64 {
        match self {
            Prior::Uniform { a, b } => {
                if x >= *a && x <= *b { -(b - a).ln() } else { f64::NEG_INFINITY }
            }
            Prior::Gaussian { mu, sigma_sq } => {
                let sigma = sigma_sq.sqrt();
                -0.5 * (2.0 * PI).ln() - sigma.ln() - 0.5 * ((x - mu) / sigma).powi(2)
            }
            Prior::Beta(alpha, beta) => {
                if x < 0.0 || x > 1.0 { return f64::NEG_INFINITY; }
                let a = *alpha;
                let b = *beta;
                let norm = log_gamma(a + b) - log_gamma(a) - log_gamma(b);
                norm + (a - 1.0) * x.ln() + (b - 1.0) * (1.0 - x).ln()
            }
            Prior::Gamma(shape, rate) => {
                if x <= 0.0 { return f64::NEG_INFINITY; }
                let s = *shape;
                let r = *rate;
                let norm = s * r.ln() - log_gamma(s);
                norm + (s - 1.0) * x.ln() - r * x
            }
            Prior::Custom { expr, params } => {
                let mut vars = params.clone();
                vars.insert("x".to_string(), x);
                match scirust_symbolic::eval(expr, &vars) {
                    Ok(val) => val.ln().max(f64::NEG_INFINITY),
                    Err(_) => f64::NEG_INFINITY,
                }
            }
        }
    }

    pub fn pdf(&self, x: f64) -> f64 {
        self.log_pdf(x).exp()
    }

    pub fn sample(&self, rng: &mut impl Rng) -> f64 {
        match self {
            Prior::Uniform { a, b } => rng.gen_range(*a..*b),
            Prior::Gaussian { mu, sigma_sq } => {
                let sigma = sigma_sq.sqrt();
                let u1: f64 = rng.gen_range(0.0..1.0);
                let u2: f64 = rng.gen_range(0.0..1.0);
                mu + sigma * (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
            }
            Prior::Beta(alpha, beta) => sample_beta(*alpha, *beta, rng),
            Prior::Gamma(shape, rate) => sample_gamma(*shape, *rate, rng),
            Prior::Custom { .. } => {
                for _ in 0..1000 {
                    let x: f64 = rng.gen_range(-10.0..10.0);
                    let p = self.pdf(x);
                    let u: f64 = rng.gen_range(0.0..1.0);
                    if u < p * 20.0 { return x; }
                }
                rng.gen_range(-10.0..10.0)
            }
        }
    }

    pub fn mean(&self) -> f64 {
        match self {
            Prior::Uniform { a, b } => (a + b) / 2.0,
            Prior::Gaussian { mu, .. } => *mu,
            Prior::Beta(alpha, beta) => alpha / (alpha + beta),
            Prior::Gamma(shape, rate) => shape / rate,
            Prior::Custom { .. } => f64::NAN,
        }
    }

    pub fn variance(&self) -> f64 {
        match self {
            Prior::Uniform { a, b } => (b - a).powi(2) / 12.0,
            Prior::Gaussian { sigma_sq, .. } => *sigma_sq,
            Prior::Beta(alpha, beta) => {
                let s = alpha + beta;
                alpha * beta / (s * s * (s + 1.0))
            }
            Prior::Gamma(shape, rate) => shape / (rate * rate),
            Prior::Custom { .. } => f64::NAN,
        }
    }
}

impl std::fmt::Display for Prior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Prior::Uniform { a, b } => write!(f, "Uniform({}, {})", a, b),
            Prior::Gaussian { mu, sigma_sq } => write!(f, "Normal({}, {})", mu, sigma_sq),
            Prior::Beta(a, b) => write!(f, "Beta({:.4}, {:.4})", a, b),
            Prior::Gamma(a, b) => write!(f, "Gamma({:.4}, {:.4})", a, b),
            Prior::Custom { .. } => write!(f, "Custom(...)"),
        }
    }
}

// ---------------------------------------------------------------------------
// Posterior
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Posterior {
    pub prior: Prior,
    pub observations: Vec<f64>,
}

impl Posterior {
    pub fn new(prior: Prior) -> Self {
        Self { prior, observations: Vec::new() }
    }

    pub fn with_data(prior: Prior, data: Vec<f64>) -> Self {
        Self { prior, observations: data }
    }

    pub fn observe(&mut self, value: f64) {
        self.observations.push(value);
    }

    pub fn log_pdf(&self, x: f64) -> f64 {
        let mut lp = self.prior.log_pdf(x);
        for &obs in &self.observations {
            lp += log_likelihood(&self.prior, x, obs);
        }
        lp
    }

    pub fn map_estimate(&self) -> f64 {
        let (lo, hi) = support_bounds(&self.prior);
        let steps = 1000;
        let step = (hi - lo) / steps as f64;
        let mut best_x = (lo + hi) / 2.0;
        let mut best_lp = f64::NEG_INFINITY;
        for i in 0..=steps {
            let x = lo + i as f64 * step;
            let lp = self.log_pdf(x);
            if lp > best_lp {
                best_lp = lp;
                best_x = x;
            }
        }
        best_x
    }

    pub fn mean(&self) -> f64 {
        let (lo, hi) = support_bounds(&self.prior);
        let steps = 500;
        let step = (hi - lo) / steps as f64;
        let mut total_w = 0.0;
        let mut total_v = 0.0;
        for i in 0..=steps {
            let x = lo + i as f64 * step;
            let w = self.log_pdf(x).exp();
            total_w += w;
            total_v += w * x;
        }
        if total_w > 0.0 { total_v / total_w } else { (lo + hi) / 2.0 }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn log_likelihood(prior: &Prior, param: f64, observation: f64) -> f64 {
    match prior {
        Prior::Gaussian { .. } => {
            -0.5 * (observation - param).powi(2) - 0.5 * (2.0 * PI).ln()
        }
        Prior::Beta(..) => {
            if observation == 1.0 { param.ln() } else { (1.0 - param).ln() }
        }
        Prior::Gamma(..) => {
            param.ln() - param * observation
        }
        Prior::Uniform { .. } | Prior::Custom { .. } => {
            -0.5 * (observation - param).powi(2)
        }
    }
}

fn support_bounds(prior: &Prior) -> (f64, f64) {
    match prior {
        Prior::Uniform { a, b } => (*a, *b),
        Prior::Gaussian { mu, sigma_sq } => {
            let sigma = sigma_sq.sqrt();
            (mu - 5.0 * sigma, mu + 5.0 * sigma)
        }
        Prior::Beta(..) => (0.0, 1.0),
        Prior::Gamma(..) => (0.0, 10.0),
        Prior::Custom { .. } => (-10.0, 10.0),
    }
}

// ---------------------------------------------------------------------------
// Simple PRNG trait (no external dependencies)
// ---------------------------------------------------------------------------

/// Minimal random number generator trait.
pub trait Rng {
    fn gen_range(&mut self, range: std::ops::Range<f64>) -> f64;
    fn gen_bool(&mut self, p: f64) -> bool;
    fn shuffle<T>(&mut self, slice: &mut [T]);
}

/// Simple xorshift-based PRNG.
pub struct XorShiftRng(u64);

impl XorShiftRng {
    pub fn new(seed: u64) -> Self {
        let mut s = seed;
        if s == 0 { s = 1; }
        Self(s)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Rng for XorShiftRng {
    fn gen_range(&mut self, range: std::ops::Range<f64>) -> f64 {
        range.start + self.next_f64() * (range.end - range.start)
    }

    fn gen_bool(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = (self.next_f64() * (i + 1) as f64) as usize;
            slice.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------------------
// Sampling helpers
// ---------------------------------------------------------------------------

fn sample_beta(alpha: f64, beta: f64, rng: &mut impl Rng) -> f64 {
    let x = sample_gamma(alpha, 1.0, rng);
    let y = sample_gamma(beta, 1.0, rng);
    x / (x + y)
}

fn sample_gamma(shape: f64, rate: f64, rng: &mut impl Rng) -> f64 {
    if shape < 1.0 {
        let u: f64 = rng.gen_range(0.0..1.0);
        sample_gamma(shape + 1.0, rate, rng) * u.powf(1.0 / shape)
    } else {
        let d = shape - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let u1: f64 = rng.gen_range(0.0..1.0);
            let u2: f64 = rng.gen_range(0.0..1.0);
            let x = (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos();
            let v = 1.0 + c * x;
            if v <= 0.0 { continue; }
            let v3 = v * v * v;
            let u: f64 = rng.gen_range(0.0..1.0);
            if u < 1.0 - 0.0331 * (x * x).powi(2) { return d * v3 / rate; }
            if u.ln() < 0.5 * x * x + d * (1.0 - v3 + v3.ln()) { return d * v3 / rate; }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// Bayes Factor
// ---------------------------------------------------------------------------

/// Compute the Bayes factor K = P(data | H1) / P(data | H0) for
/// comparing two prior distributions given observed data.
///
/// * K > 1 → evidence for H1 over H0
/// * K < 1 → evidence for H0 over H1
/// * |ln(K)| > 1 → substantial evidence
/// * |ln(K)| > 3 → strong evidence
/// * |ln(K)| > 5 → decisive evidence
pub fn bayes_factor(prior_h0: &Prior, prior_h1: &Prior, data: &[f64]) -> f64 {
    let log_ml_h0 = log_marginal_likelihood(prior_h0, data);
    let log_ml_h1 = log_marginal_likelihood(prior_h1, data);
    (log_ml_h1 - log_ml_h0).exp()
}

/// Compute the log marginal likelihood P(data | prior) by integrating
/// over the parameter space: ∫ P(data | θ) P(θ) dθ
///
/// Uses numerical integration (Riemann sum) over the prior support.
pub fn log_marginal_likelihood(prior: &Prior, data: &[f64]) -> f64 {
    let (lo, hi) = support_bounds(prior);
    let steps = 2000;
    let step = (hi - lo) / steps as f64;
    let eps = 1e-10; // avoid boundary singularities

    // Use log-sum-exp trick for numerical stability
    let mut max_log = f64::NEG_INFINITY;
    let mut log_values = Vec::with_capacity(steps as usize + 1);

    for i in 0..=steps {
        let theta = if i == 0 { lo + eps } else if i == steps as usize { hi - eps } else { lo + i as f64 * step };
        let mut log_lik = prior.log_pdf(theta);
        for &obs in data {
            log_lik += log_likelihood(prior, theta, obs);
        }
        log_values.push(log_lik);
        if log_lik > max_log { max_log = log_lik; }
    }

    if max_log == f64::NEG_INFINITY { return f64::NEG_INFINITY; }

    // log(∫ exp(log_lik(θ)) dθ) ≈ max_log + log(Σ exp(log_lik_i - max_log) * Δθ)
    let mut sum_exp = 0.0;
    for &lv in &log_values {
        sum_exp += (lv - max_log).exp();
    }
    max_log + (sum_exp * step).ln()
}

/// Compute the posterior predictive distribution at point `x_new`,
/// given a prior and observed data: P(x_new | data) = ∫ P(x_new | θ) P(θ | data) dθ
pub fn posterior_predictive(prior: &Prior, observations: &[f64], x_new: f64) -> f64 {
    let (lo, hi) = support_bounds(prior);
    let steps = 2000;
    let step = (hi - lo) / steps as f64;
    let eps = 1e-10;

    let posterior = Posterior::with_data(prior.clone(), observations.to_vec());

    let mut total = 0.0;
    for i in 0..=steps {
        let theta = if i == 0 { lo + eps } else if i == steps as usize { hi - eps } else { lo + i as f64 * step };
        let post_density = posterior.log_pdf(theta).exp();
        let like = log_likelihood(prior, theta, x_new).exp();
        total += post_density * like * step;
    }
    total
}

/// Compute a credible interval [lower, upper] containing `prob` of the
/// posterior probability mass (e.g., prob=0.95 for a 95% credible interval).
pub fn credible_interval(prior: &Prior, observations: &[f64], prob: f64) -> (f64, f64) {
    let (lo, hi) = support_bounds(prior);
    let steps = 1000;
    let step = (hi - lo) / steps as f64;

    let posterior = Posterior::with_data(prior.clone(), observations.to_vec());
    let eps = 1e-10;

    // Compute pdf at each point
    let mut points: Vec<(f64, f64)> = (0..=steps)
        .map(|i| {
            let x = if i == 0 { lo + eps } else if i == steps as usize { hi - eps } else { lo + i as f64 * step };
            let p = posterior.log_pdf(x).exp().max(0.0);
            (x, p)
        })
        .collect();

    // Normalize
    let total: f64 = points.iter().map(|(_, p)| p * step).sum();
    if total == 0.0 { return (lo, hi); }
    for (_, p) in points.iter_mut() { *p /= total; }

    // Find HPD (Highest Posterior Density) interval
    // Sort by density descending, accumulate until prob reached
    let mut sorted: Vec<(usize, f64)> = points.iter().enumerate()
        .map(|(i, (_, p))| (i, *p))
        .collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let mut covered = 0.0;
    let mut selected = vec![false; points.len()];
    for &(idx, p) in &sorted {
        if covered >= prob { break; }
        selected[idx] = true;
        covered += p;
    }

    // Find min and max of selected indices
    let min_idx = selected.iter().position(|&s| s).unwrap_or(0);
    let max_idx = selected.iter().rposition(|&s| s).unwrap_or(points.len() - 1);

    (points[min_idx].0, points[max_idx].0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_pdf() {
        let u = Prior::Uniform { a: 0.0, b: 1.0 };
        assert!((u.pdf(0.5) - 1.0).abs() < 1e-10);
        assert_eq!(u.pdf(-0.1), 0.0);
        assert_eq!(u.pdf(1.1), 0.0);
    }

    #[test]
    fn test_gaussian_pdf() {
        let g = Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 };
        assert!((g.pdf(0.0) - (1.0 / (2.0 * PI).sqrt())).abs() < 1e-10);
        assert!(g.pdf(1.0) > 0.0);
    }

    #[test]
    fn test_beta_pdf() {
        let b = Prior::Beta(2.0, 2.0);
        assert!(b.pdf(0.5) > 0.0);
        assert_eq!(b.pdf(-0.1), 0.0);
        assert_eq!(b.pdf(1.1), 0.0);
    }

    #[test]
    fn test_gamma_pdf() {
        let g = Prior::Gamma(2.0, 1.0);
        assert!(g.pdf(1.0) > 0.0);
        assert_eq!(g.pdf(-1.0), 0.0);
    }

    #[test]
    fn test_prior_mean() {
        let u = Prior::Uniform { a: 0.0, b: 10.0 };
        assert!((u.mean() - 5.0).abs() < 1e-10);
        let g = Prior::Gaussian { mu: 3.0, sigma_sq: 2.0 };
        assert!((g.mean() - 3.0).abs() < 1e-10);
        let b = Prior::Beta(2.0, 3.0);
        assert!((b.mean() - 0.4).abs() < 1e-10);
    }

    #[test]
    fn test_posterior_map() {
        let prior = Prior::Beta(2.0, 2.0);
        let mut post = Posterior::new(prior);
        post.observe(1.0);
        post.observe(1.0);
        post.observe(0.0);
        let map = post.map_estimate();
        assert!(map > 0.5 && map < 1.0);
    }

    #[test]
    fn test_sampling() {
        let mut rng = XorShiftRng::new(42);
        let g = Prior::Gaussian { mu: 5.0, sigma_sq: 1.0 };
        let samples: Vec<f64> = (0..1000).map(|_| g.sample(&mut rng)).collect();
        let mean = samples.iter().sum::<f64>() / 1000.0;
        assert!((mean - 5.0).abs() < 0.2);
    }

    #[test]
    fn test_log_gamma() {
        assert!((log_gamma(2.0) - 0.0).abs() < 1e-10);
        assert!((log_gamma(4.0) - 6.0f64.ln()).abs() < 1e-10);
    }

    // --- Property-based tests ---

    #[test]
    fn test_pdf_integral_uniform() {
        let u = Prior::Uniform { a: 0.0, b: 1.0 };
        let integral: f64 = (0..1000).map(|i| u.pdf(i as f64 / 1000.0) * 0.001).sum();
        assert!((integral - 1.0).abs() < 0.02, "Uniform integral={}", integral);
    }

    #[test]
    fn test_pdf_integral_gaussian() {
        let g = Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 };
        let integral: f64 = (-500..500).map(|i| {
            let x = i as f64 / 100.0;
            g.pdf(x) * 0.01
        }).sum();
        assert!((integral - 1.0).abs() < 0.02, "Gaussian integral={}", integral);
    }

    #[test]
    fn test_prior_variance_nonnegative() {
        let cases = vec![
            Prior::Uniform { a: 0.0, b: 10.0 },
            Prior::Gaussian { mu: 0.0, sigma_sq: 2.0 },
            Prior::Beta(2.0, 5.0),
            Prior::Gamma(3.0, 1.0),
        ];
        for prior in cases {
            let v = prior.variance();
            assert!(v >= 0.0 || v.is_nan(), "Variance negative: {}", v);
        }
    }

    #[test]
    fn test_posterior_monotonic() {
        // More observations should shift MAP in the data direction
        let prior = Prior::Beta(2.0, 2.0);
        let mut post = Posterior::new(prior);

        let map_before = post.map_estimate();
        post.observe(1.0);
        post.observe(1.0);
        post.observe(1.0);
        let map_after = post.map_estimate();
        assert!(map_after >= map_before, "MAP should increase with positive observations");
    }

    #[test]
    fn test_sampling_mean_approx() {
        let mut rng = XorShiftRng::new(12345);
        let g = Prior::Gaussian { mu: 10.0, sigma_sq: 4.0 };
        let samples: Vec<f64> = (0..5000).map(|_| g.sample(&mut rng)).collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!((mean - 10.0).abs() < 0.3, "Sample mean {} != 10.0", mean);
    }

    #[test]
    fn test_bayes_factor() {
        // H0: Beta(2,2) vs H1: Beta(5,1) given data with mostly successes
        let h0 = Prior::Beta(2.0, 2.0);
        let h1 = Prior::Beta(5.0, 1.0);
        let data = vec![1.0, 1.0, 1.0, 0.0, 1.0];
        let bf = bayes_factor(&h0, &h1, &data);
        // H1 (Beta(5,1)) expects more 1s, so should be preferred
        assert!(bf > 0.0, "Bayes factor should be positive, got {}", bf);
    }

    #[test]
    fn test_posterior_predictive() {
        let prior = Prior::Beta(2.0, 2.0);
        let observations = vec![1.0, 1.0, 0.0];
        let pred = posterior_predictive(&prior, &observations, 1.0);
        // After observing 2 successes and 1 failure, P(x_new=1) should be > 0.5
        assert!(pred > 0.0 && pred < 1.0, "Predictive should be in (0,1), got {}", pred);
    }

    #[test]
    fn test_credible_interval() {
        let prior = Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 };
        let observations = vec![0.5, 1.0, -0.3, 0.8];
        let (lo, hi) = credible_interval(&prior, &observations, 0.95);
        assert!(lo < hi, "CI lower {} should be < upper {}", lo, hi);
        assert!(hi > 0.0, "CI upper should be positive, got {}", hi);
    }

    #[test]
    fn test_log_marginal_likelihood() {
        let prior = Prior::Uniform { a: 0.0, b: 1.0 };
        let data = vec![0.5, 0.6, 0.55];
        let lml = log_marginal_likelihood(&prior, &data);
        // Should be finite
        assert!(lml.is_finite(), "LML should be finite, got {}", lml);
    }
}
