use scirust::Tensor;

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
        Self {
            projection_matrix: Tensor::from_slice(&weights),
            bias: Tensor::from_slice(&biases),
        }
    }

    pub fn compute_chemical_levels(&self, pad_tensor: &Tensor) -> NeurochemistryProfile {
        let pad = pad_tensor.as_slice();
        let weights = self.projection_matrix.as_slice();
        let bias = self.bias.as_slice();

        let mut results = [0.0f32; 3];
        for i in 0..3 {
            let mut sum = 0.0;
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
