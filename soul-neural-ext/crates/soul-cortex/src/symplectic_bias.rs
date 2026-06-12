use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref BIAS_DURATION: prometheus::Histogram = prometheus::register_histogram!(
        "symplectic_bias_duration_seconds",
        "Duration of apply_symplectic_bias in seconds",
        vec![1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 5e-3]
    ).unwrap();
    static ref BIAS_APPLIED: prometheus::IntCounter = prometheus::register_int_counter!(
        "symplectic_bias_applied_total",
        "Number of times symplectic bias was applied"
    ).unwrap();
    static ref BIAS_TEMPERATURE: prometheus::Gauge = prometheus::register_gauge!(
        "symplectic_bias_temperature_current",
        "Current temperature of the symplectic bias"
    ).unwrap();
    static ref STATE: Mutex<ProjectState> = Mutex::new(ProjectState::default());
}

struct ProjectState {
    w_proj: Vec<f32>,
    dim_hnn: usize,
    dim_emb: usize,
    top_k: usize,
    temperature: f64,
    enabled: bool,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self { w_proj: vec![], dim_hnn: 1, dim_emb: 1, top_k: 10, temperature: 1.0, enabled: false }
    }
}

pub fn init_projection(w_proj: Vec<f32>, dim_hnn: usize, dim_emb: usize, temperature: f64, top_k: usize) {
    let mut state = STATE.lock().unwrap();
    state.w_proj = w_proj;
    state.dim_hnn = dim_hnn;
    state.dim_emb = dim_emb;
    state.top_k = top_k;
    state.temperature = temperature;
    state.enabled = true;
    BIAS_TEMPERATURE.set(temperature);
}

pub fn is_enabled() -> bool {
    STATE.lock().unwrap().enabled
}

pub fn reset() {
    let mut state = STATE.lock().unwrap();
    state.enabled = false;
    state.temperature = 1.0;
}

pub fn apply_symplectic_bias(
    mut logits: Vec<f64>,
    state_q: &[f64],
    state_p: &[f64],
) -> Vec<f64> {
    let _timer = BIAS_DURATION.start_timer();

    let state = STATE.lock().unwrap();
    if !state.enabled || state.temperature <= 1e-10 || logits.is_empty() {
        return logits;
    }

    let dim_hnn = state.dim_hnn;
    let _dim_emb = state.dim_emb;
    let w_proj = state.w_proj.clone(); // clone to avoid borrow issues with MutexGuard
    let top_k = state.top_k.min(logits.len());
    let temperature = state.temperature;
    drop(state);

    let mut hnn_vec: Vec<f64> = Vec::with_capacity(2 * dim_hnn);
    for &x in state_q.iter().take(dim_hnn) { hnn_vec.push(x); }
    for &x in state_p.iter().take(dim_hnn) { hnn_vec.push(x); }
    let norm: f64 = hnn_vec.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm < 1e-10 { return logits; }
    for v in &mut hnn_vec { *v /= norm; }

    let proj_len = hnn_vec.len().min(w_proj.len());
    let mut hnn_proj = vec![0.0f64; proj_len];
    for i in 0..hnn_proj.len() {
        let mut sum = 0.0;
        let row_start = i * (2 * dim_hnn);
        for j in 0..(2 * dim_hnn).min(hnn_vec.len()) {
            sum += *w_proj.get(row_start + j).unwrap_or(&0.0) as f64 * hnn_vec[j];
        }
        hnn_proj[i] = sum;
    }

    let mut indexed: Vec<(usize, f64)> = logits.iter().cloned().enumerate().collect();
    indexed.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    indexed.truncate(top_k);

    let bias_scale = temperature.clamp(0.0, 10.0);
    for &(token_id, _) in &indexed {
        // Calcul d'une similarité basée sur la projection HNN
        // (version simplifiée : les vrais embeddings token seront ajoutés via Embedder)
        let token_hash = (token_id as f64 * 0.1).sin() * 0.5 + 0.5;
        let state_hash = (hnn_proj.iter().sum::<f64>() * 0.1).sin() * 0.5 + 0.5;
        let cos_sim = (token_hash + state_hash) * 0.5;
        let bias = bias_scale * cos_sim;
        logits[token_id] = (logits[token_id] + bias).clamp(-1e4, 1e4);
    }

    BIAS_APPLIED.inc();
    logits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_bias_when_disabled() {
        reset();
        let logits = vec![1.0, 2.0, 3.0];
        let r = apply_symplectic_bias(logits.clone(), &[0.5], &[0.3]);
        assert_eq!(r, logits);
    }

    #[test]
    fn test_zero_temperature() {
        reset();
        init_projection(vec![1.0; 6], 1, 3, 0.0, 2);
        let logits = vec![1.0, 2.0, 3.0];
        let r = apply_symplectic_bias(logits.clone(), &[0.5], &[0.3]);
        assert_eq!(r, logits);
    }

    #[test]
    fn test_degenerate_state() {
        reset();
        init_projection(vec![1.0; 6], 1, 3, 1.0, 2);
        let logits = vec![1.0, 2.0, 3.0];
        let r = apply_symplectic_bias(logits.clone(), &[0.0], &[0.0]);
        assert_eq!(r, logits);
    }

    #[test]
    fn test_bias_applied() {
        reset();
        init_projection(vec![1.0; 6], 1, 3, 1.0, 2);
        let logits = vec![10.0, 1.0, 0.5];
        let r = apply_symplectic_bias(logits, &[0.5], &[0.3]);
        assert!(r[0] != 10.0 || r[1] != 1.0);
    }

    #[test]
    fn test_clamp_no_inf() {
        reset();
        init_projection(vec![1e5; 6], 1, 3, 1e5, 3);
        let logits = vec![1e10, -1e10, 0.0];
        let r = apply_symplectic_bias(logits, &[1.0], &[1.0]);
        for &v in &r { assert!(v.is_finite(), "should be finite, got {}", v); }
    }
}
