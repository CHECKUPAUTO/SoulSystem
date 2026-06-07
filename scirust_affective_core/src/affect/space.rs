use std::time::Instant;
use parking_lot::RwLock;
pub struct Tensor { pub data: Vec<f32>, pub dimensions: Vec<usize> }
impl Tensor { pub fn new_vector(data: Vec<f32>) -> Self { let len = data.len(); Self { data, dimensions: vec![len] } } }
impl Default for AffectiveState {
    fn default() -> Self { Self::new() }
}

pub struct AffectiveState { coordinates: RwLock<Tensor>, birth_time: Instant }
impl AffectiveState {
    pub fn new() -> Self { Self { coordinates: RwLock::new(Tensor::new_vector(vec![0.0, 0.0, 0.0])), birth_time: Instant::now() } }
    pub fn decay_towards_baseline(&self, dt: f32, baseline: &[f32; 3], decay_rates: &[f32; 3]) {
        let mut coords_lock = self.coordinates.write(); let data = &mut coords_lock.data;
        let factor0 = (-decay_rates[0] * dt).exp(); data[0] = baseline[0] + (data[0] - baseline[0]) * factor0;
        let factor1 = (-decay_rates[1] * dt).exp(); data[1] = baseline[1] + (data[1] - baseline[1]) * factor1;
        let factor2 = (-decay_rates[2] * dt).exp(); data[2] = baseline[2] + (data[2] - baseline[2]) * factor2;
        for val in data.iter_mut() { *val = val.clamp(-1.0, 1.0); }
    }
    pub fn get_coordinates(&self) -> [f32; 3] { let coords_lock = self.coordinates.read(); [coords_lock.data[0], coords_lock.data[1], coords_lock.data[2]] }
    pub fn uptime_ns(&self) -> u64 { self.birth_time.elapsed().as_nanos() as u64 }
    pub fn apply_stimulus(&self, stimulus: &Tensor) {
        let mut coords_lock = self.coordinates.write(); let data = &mut coords_lock.data;
        if stimulus.data.len() >= 3 {
            for i in 0..3 { data[i] += stimulus.data[i]; }
        }
        for val in data.iter_mut() { *val = val.clamp(-1.0, 1.0); }
    }
}
