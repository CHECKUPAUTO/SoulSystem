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

impl NeuromodulatorMapper {
    /// Entraine W (3x3) et b (3) par moindres carres regularises (ridge) a partir
    /// d'echantillons (PAD 3D -> neurochimie 3D cible). Pour chaque sortie i, resout
    /// le systeme normal augmente [x|1] (3 poids + 1 biais) par Gauss a pivot partiel.
    /// Renvoie le MSE final (sortie clampee). NB: machinerie reelle et testee, mais
    /// un VRAI jeu (input -> cible) reste a fournir pour que ce soit utile.
    #[allow(clippy::needless_range_loop)]
pub fn fit(&mut self, inputs: &[[f32; 3]], targets: &[[f32; 3]], ridge: f32) -> f32 {
        assert_eq!(
            inputs.len(),
            targets.len(),
            "inputs/targets de meme longueur"
        );
        let n = inputs.len();
        assert!(n > 0, "jeu d'entrainement vide");

        let mut ata = [[0.0f64; 4]; 4];
        for s in 0..n {
            let x = [
                inputs[s][0] as f64,
                inputs[s][1] as f64,
                inputs[s][2] as f64,
                1.0,
            ];
            for r in 0..4 {
                for c in 0..4 {
                    ata[r][c] += x[r] * x[c];
                }
            }
        }
        for d in 0..4 {
            ata[d][d] += ridge as f64;
        }

        let mut w = [0.0f32; 9];
        let mut bias = [0.0f32; 3];
        for i in 0..3 {
            let mut rhs = [0.0f64; 4];
            for s in 0..n {
                let x = [
                    inputs[s][0] as f64,
                    inputs[s][1] as f64,
                    inputs[s][2] as f64,
                    1.0,
                ];
                let y = targets[s][i] as f64;
                for r in 0..4 {
                    rhs[r] += x[r] * y;
                }
            }
            let theta = solve4(ata, rhs);
            w[i * 3] = theta[0] as f32;
            w[i * 3 + 1] = theta[1] as f32;
            w[i * 3 + 2] = theta[2] as f32;
            bias[i] = theta[3] as f32;
        }

        self.projection_matrix = Tensor::from_vec(w.to_vec(), 1, 9);
        self.bias = Tensor::from_vec(bias.to_vec(), 1, 3);

        let mut mse = 0.0f32;
        for s in 0..n {
            let prof = self.compute_chemical_levels(&Tensor::from_vec(inputs[s].to_vec(), 1, 3));
            let pred = [prof.dopamine, prof.noradrenaline, prof.serotonin];
            for i in 0..3 {
                let d = pred[i] - targets[s][i];
                mse += d * d;
            }
        }
        mse / (n as f32 * 3.0)
    }
}

/// Resout A x = b (4x4) par elimination de Gauss a pivot partiel.
#[allow(clippy::needless_range_loop)]
fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> [f64; 4] {
    for col in 0..4 {
        let mut piv = col;
        for r in (col + 1)..4 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let d = a[col][col];
        if d.abs() < 1e-12 {
            continue;
        }
        for r in 0..4 {
            if r == col {
                continue;
            }
            let f = a[r][col] / d;
            for c in col..4 {
                a[r][c] -= f * a[col][c];
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = [0.0f64; 4];
    for i in 0..4 {
        let d = a[i][i];
        x[i] = if d.abs() < 1e-12 { 0.0 } else { b[i] / d };
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_recovers_known_mapping() {
        let wt = [0.5, 0.1, 0.0, 0.0, 0.4, 0.1, 0.1, 0.0, 0.3f32];
        let bt = [0.10, 0.20, 0.05f32];
        let xs: Vec<[f32; 3]> = (0..240)
            .map(|k| {
                [
                    ((k * 7) % 10) as f32 / 10.0,
                    ((k * 3 + 1) % 10) as f32 / 10.0,
                    ((k * 5 + 2) % 10) as f32 / 10.0,
                ]
            })
            .collect();
        let ys: Vec<[f32; 3]> = xs
            .iter()
            .map(|x| {
                let mut y = [0.0f32; 3];
                for i in 0..3 {
                    y[i] = (wt[i * 3] * x[0] + wt[i * 3 + 1] * x[1] + wt[i * 3 + 2] * x[2] + bt[i])
                        .clamp(0.0, 1.0);
                }
                y
            })
            .collect();

        let mut m = NeuromodulatorMapper::new(vec![0.0; 9], vec![0.0; 3]);
        let mse = m.fit(&xs, &ys, 1e-6);
        assert!(mse < 1e-4, "MSE trop eleve apres fit: {}", mse);
        let w = &m.projection_matrix.data;
        for i in 0..9 {
            assert!(
                (w[i] - wt[i]).abs() < 0.05,
                "poids {} = {} (attendu {})",
                i,
                w[i],
                wt[i]
            );
        }
        println!(
            "PREUVE fit : MSE={:.2e}, W recupere a +-0.05 du vrai mapping",
            mse
        );
    }
}
