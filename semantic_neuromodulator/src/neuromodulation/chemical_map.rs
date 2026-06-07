use scirust::autodiff::reverse::Tensor;

#[repr(C, align(64))]
pub struct NeurochemistryProfile {
    pub dopamine: f32,
    pub noradrenaline: f32,
    pub serotonin: f32,
}

pub struct NeuromodulatorMapper {
    pub projection_matrix: Tensor,
    pub bias: Tensor,
}

impl NeuromodulatorMapper {
    pub fn new(weights: Vec<f32>, biases: Vec<f32>) -> Self {
        let w_len = weights.len();
        let b_len = biases.len();
        Self {
            projection_matrix: Tensor::from_vec(weights, 1, w_len),
            bias: Tensor::from_vec(biases, 1, b_len),
        }
    }

    pub fn compute_chemical_levels(&self, pad_tensor: &Tensor) -> NeurochemistryProfile {
        let pad = &pad_tensor.data;
        let weights = &self.projection_matrix.data;
        let bias = &self.bias.data;

        let mut results = [0.0f32; 3];
        for i in 0..3 {
            let mut sum = 0.0f32;
            for j in 0..3 {
                sum += pad[j] * weights[i * 3 + j];
            }
            results[i] = (sum + bias[i]).clamp(0.0, 1.0);
        }

        NeurochemistryProfile {
            dopamine: results[0],
            noradrenaline: results[1],
            serotonin: results[2],
        }
    }
}
