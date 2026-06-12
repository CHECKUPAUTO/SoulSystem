// ==========================================================================
// observer.rs — Système d'Observation Latente (Option C)
//
// Real-time PCA on the M latent buffer from PTNLPerceiver at each step.
// Exports coordinates via mpsc channel for external 3D visualisation
// (Three.js / Plotly).
//
// Tracks attractor variance: contraction = narrative confidence,
// expansion = uncertainty/exploration.
// ==========================================================================

use std::sync::mpsc;
use rand::Rng;

// --------------------------------------------------------------------------
// LatentObservation — single step snapshot
// --------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct LatentObservation {
    pub step: usize,
    /// PCA-reduced 3D coordinates
    pub pca_x: f64,
    pub pca_y: f64,
    pub pca_z: f64,
    /// Variance explained by top 3 components
    pub explained_var: f64,
    /// Total variance of the latent buffer (attractor spread)
    pub total_variance: f64,
    /// Raw shape info
    pub m: usize,
    pub d_latent: usize,
}

// --------------------------------------------------------------------------
// RealTimePCA — streaming PCA via online covariance estimation
// --------------------------------------------------------------------------

pub struct RealTimePCA {
    pub n_components: usize,
    pub d_latent: usize,
    // Running mean
    pub mean: Vec<f64>,
    // Running covariance (flattened upper-triangular)
    pub cov: Vec<f64>,
    // Count of observations seen
    pub n_obs: f64,
}

impl RealTimePCA {
    pub fn new(n_components: usize, d_latent: usize) -> Self {
        let cov_size = d_latent * d_latent;
        Self {
            n_components,
            d_latent,
            mean: vec![0.0; d_latent],
            cov: vec![0.0; cov_size],
            n_obs: 0.0,
        }
    }

    /// Update running statistics with a new latent buffer (M × d_latent).
    /// Returns the PCA-reduced 3D coordinates and variance metrics.
    pub fn update(&mut self, latents: &[f64], m: usize, d_latent: usize) -> LatentObservation {
        debug_assert_eq!(latents.len(), m * d_latent);

        // Compute mean of current batch
        let mut batch_mean = vec![0.0; d_latent];
        for i in 0..m {
            for j in 0..d_latent {
                batch_mean[j] += latents[i * d_latent + j];
            }
        }
        for j in 0..d_latent {
            batch_mean[j] /= m as f64;
        }

        // Update running mean (Welford-style)
        if self.n_obs == 0.0 {
            self.mean.copy_from_slice(&batch_mean);
        } else {
            let total = self.n_obs + m as f64;
            for j in 0..d_latent {
                let delta = batch_mean[j] - self.mean[j];
                self.mean[j] += delta * (m as f64 / total);
            }
        }

        // Update running covariance (simplified online)
        // For each observation, compute outer product of centered vector
        let alpha = if self.n_obs == 0.0 { 1.0 } else { self.n_obs / (self.n_obs + m as f64) };
        for i in 0..m {
            let mut centered = vec![0.0; d_latent];
            for j in 0..d_latent {
                centered[j] = latents[i * d_latent + j] - self.mean[j];
            }
            // Outer product: centered ⊗ centered
            for j in 0..d_latent {
                for k in 0..d_latent {
                    let idx = j * d_latent + k;
                    self.cov[idx] = self.cov[idx] * alpha + centered[j] * centered[k] * (1.0 - alpha);
                }
            }
        }

        self.n_obs += m as f64;

        // PCA via power iteration on top 3 components
        let components = self.power_iteration(d_latent);

        // Project current batch mean onto components
        let mut scores = vec![0.0; self.n_components];
        for i in 0..self.n_components {
            for j in 0..d_latent {
                scores[i] += batch_mean[j] * components[i][j];
            }
        }

        // Compute eigenvalues (variance explained per component)
        let mut eigenvalues = vec![0.0; self.n_components];
        for i in 0..self.n_components {
            for j in 0..d_latent {
                for k in 0..d_latent {
                    eigenvalues[i] += components[i][j] * self.cov[j * d_latent + k] * components[i][k];
                }
            }
        }

        let total_var: f64 = (0..d_latent).map(|j| self.cov[j * d_latent + j]).sum();
        let explained: f64 = eigenvalues.iter().sum();

        // Pad to 3 components (PCA may return fewer than n_components if underdetermined)
        let coords = if scores.len() >= 3 {
            (scores[0], scores[1], scores[2])
        } else {
            (scores.first().copied().unwrap_or(0.0),
             scores.get(1).copied().unwrap_or(0.0),
             0.0)
        };

        LatentObservation {
            step: 0,
            pca_x: coords.0,
            pca_y: coords.1,
            pca_z: coords.2,
            explained_var: explained / total_var.max(1e-12),
            total_variance: total_var,
            m,
            d_latent,
        }
    }

    /// Power iteration to find top-k eigenvectors.
    fn power_iteration(&self, d: usize) -> Vec<Vec<f64>> {
        let mut rng = rand::thread_rng();
        let mut components = Vec::with_capacity(self.n_components);
        let mut residual = self.cov.clone();

        for _ in 0..self.n_components {
            // Initial random vector
            let mut v: Vec<f64> = (0..d).map(|_| rng.gen::<f64>() * 2.0 - 1.0).collect();
            let mut norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm < 1e-12 { norm = 1.0; }
            for x in v.iter_mut() { *x /= norm; }

            // Power iteration (5 iterations is enough for streaming approximation)
            for _ in 0..5 {
                let mut w = vec![0.0; d];
                for i in 0..d {
                    for j in 0..d {
                        w[i] += residual[i * d + j] * v[j];
                    }
                }
                let w_norm: f64 = w.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
                for i in 0..d {
                    v[i] = w[i] / w_norm;
                }
            }

            components.push(v.clone());

            // Deflate: subtract explained component from residual
            let lambda: f64 = (0..d).map(|i| {
                (0..d).map(|j| v[i] * self.cov[i * d + j] * v[j]).sum::<f64>()
            }).sum();
            for i in 0..d {
                for j in 0..d {
                    residual[i * d + j] -= lambda * v[i] * v[j];
                }
            }
        }

        components
    }
}

// --------------------------------------------------------------------------
// Observer — high-level orchestrator
// --------------------------------------------------------------------------

pub struct LatentObserver {
    pub pca: RealTimePCA,
    pub sender: mpsc::Sender<LatentObservation>,
    pub receiver: mpsc::Receiver<LatentObservation>,
    pub step_counter: usize,
}

impl LatentObserver {
    pub fn new(d_latent: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            pca: RealTimePCA::new(3, d_latent),
            sender: tx,
            receiver: rx,
            step_counter: 0,
        }
    }

    /// Observe the current latent buffer, compute PCA, and push to channel.
    pub fn observe(&mut self, latents: &[f64], m: usize, d_latent: usize) {
        let mut obs = self.pca.update(latents, m, d_latent);
        obs.step = self.step_counter;
        self.step_counter += 1;

        // Send to channel (non-blocking; drop if full)
        let _ = self.sender.send(obs);
    }

    /// Export all buffered observations as CSV string.
    pub fn export_csv(&self) -> String {
        let _wtr = csv::Writer::from_writer(Vec::new());
        // Drain receiver
        let obs_list: Vec<LatentObservation> = self.receiver.try_iter().collect();
        // We can't easily write to a string with csv crate without writer ref
        // So we use a simpler approach
        let mut csv = String::from("step,pca_x,pca_y,pca_z,explained_var,total_variance,m,d_latent\n");
        for obs in &obs_list {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                obs.step, obs.pca_x, obs.pca_y, obs.pca_z,
                obs.explained_var, obs.total_variance, obs.m, obs.d_latent
            ));
        }
        csv
    }

    /// Export as JSON array string.
    pub fn export_json(&self) -> String {
        let obs_list: Vec<LatentObservation> = self.receiver.try_iter().collect();
        serde_json::to_string_pretty(&obs_list).unwrap_or_else(|_| String::from("[]"))
    }
}
