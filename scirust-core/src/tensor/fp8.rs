//! FP8 (E4M3/E5M2) dense layout for Blackwell Transformer Engine.

#[repr(align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Fp8Tensor {
    pub data: [u8; 8], // Dense block of 8 elements
}

impl Fp8Tensor {
    pub fn from_f32(vals: [f32; 8]) -> Self {
        let mut data = [0u8; 8];
        for i in 0..8 {
            // Software emulation: Simple truncation or mapping to E4M3
            data[i] = (vals[i].abs() as u8) & 0xFF;
        }
        Self { data }
    }

    pub fn to_f32(&self) -> [f32; 8] {
        let mut res = [0.0f32; 8];
        for i in 0..8 {
            res[i] = self.data[i] as f32;
        }
        res
    }
}
