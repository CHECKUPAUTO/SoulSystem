//! FP4 (E2M1) dense layout for Blackwell architecture.

#[repr(align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Fp4Tensor {
    pub data: [u8; 4], // 4 bytes packing 8 FP4 elements (4 bits each)
}

impl Fp4Tensor {
    pub fn pack(vals: [f32; 8]) -> Self {
        let mut data = [0u8; 4];
        for i in 0..4 {
            let low = (vals[i * 2] as u8) & 0x0F;
            let high = (vals[i * 2 + 1] as u8) & 0x0F;
            data[i] = low | (high << 4);
        }
        Self { data }
    }
}
