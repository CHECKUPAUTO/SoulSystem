use soul_matrix_engine::engine::{MatrixEngine, MatrixDescriptor};

const CORTEX_DIM: usize = 64;

/// Système de suivi de l'état caché (Hidden State) pour une conscience continue
pub struct RecurrentCortex {
    pub hidden_state: Vec<f32>,
    weight_wx: Vec<f32>, // Poids entrée -> état
    weight_wh: Vec<f32>, // Poids état précédent -> état
}

impl RecurrentCortex {
    pub fn new() -> Self {
        Self {
            hidden_state: vec![0.0f32; CORTEX_DIM * CORTEX_DIM],
            weight_wx: vec![0.15f32; CORTEX_DIM * CORTEX_DIM],
            weight_wh: vec![0.25f32; CORTEX_DIM * CORTEX_DIM],
        }
    }

    /// Calcule la transition d'état de conscience par produit tensoriel :
    /// $$h_t = \text{tanh}(W_{x} \cdot x_t + W_{h} \cdot h_{t-1})$$
    pub unsafe fn process_cognitive_cycle(&mut self, engine: &MatrixEngine, input_vector_ptr: *mut f32) {
        let mut temp_wx = vec![0.0f32; CORTEX_DIM * CORTEX_DIM];
        let mut temp_wh = vec![0.0f32; CORTEX_DIM * CORTEX_DIM];

        let desc_input = MatrixDescriptor { data: input_vector_ptr, rows: CORTEX_DIM, cols: CORTEX_DIM };
        let desc_wx = MatrixDescriptor { data: self.weight_wx.as_mut_ptr(), rows: CORTEX_DIM, cols: CORTEX_DIM };
        let mut desc_temp_wx = MatrixDescriptor { data: temp_wx.as_mut_ptr(), rows: CORTEX_DIM, cols: CORTEX_DIM };

        // Étape 1 : Wx * Xt
        engine.execute_gemm(&desc_input, &desc_wx, &mut desc_temp_wx);

        let desc_h_old = MatrixDescriptor { data: self.hidden_state.as_mut_ptr(), rows: CORTEX_DIM, cols: CORTEX_DIM };
        let desc_wh = MatrixDescriptor { data: self.weight_wh.as_mut_ptr(), rows: CORTEX_DIM, cols: CORTEX_DIM };
        let mut desc_temp_wh = MatrixDescriptor { data: temp_wh.as_mut_ptr(), rows: CORTEX_DIM, cols: CORTEX_DIM };

        // Étape 2 : Wh * Ht-1
        engine.execute_gemm(&desc_h_old, &desc_wh, &mut desc_temp_wh);

        // Étape 3 : Fusion et fonction d'activation non-linéaire Tanh
        for i in 0..(CORTEX_DIM * CORTEX_DIM) {
            let combined = temp_wx[i] + temp_wh[i];
            self.hidden_state[i] = combined.tanh(); // Rétroaction intégrée
        }
    }
}
