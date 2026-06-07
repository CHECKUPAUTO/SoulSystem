//! ChaCha8 stream cipher as a deterministic random number generator.
//!
//! Replaces `rand_chacha` + `rand_distr` — no external dependencies.

/// ChaCha8 internal state (16 × u32).
#[derive(Clone)]
pub struct ChaCha8Rng {
    state: [u32; 16],
}

impl ChaCha8Rng {
    /// Seed from a single u64. Uses the standard ChaCha key schedule with
    /// `"expand 32-byte k"` constant block as base and seeds the key half.
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut state = [0u32; 16];
        // "expand 32-byte k" constant block
        state[0]  = 0x61707865;
        state[1]  = 0x3320646e;
        state[2]  = 0x79622d32;
        state[3]  = 0x6b206574;
        // Counter starts at 0 (words 4-7)
        // Seed key words (words 8-11)
        state[8]  = seed as u32;
        state[9]  = (seed >> 32) as u32;
        // Remaining words are zero (words 10-15)
        Self { state }
    }

    /// ChaCha20 quarter round.
    #[inline]
    fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
        const ROTATE: [u32; 4] = [16, 12, 8, 7];
        for i in 0..4 {
            s[a] = s[a].wrapping_add(s[b]);
            s[d] ^= s[a];
            let rot = ROTATE[i];
            s[d] = (s[d] << rot) | (s[d] >> (32 - rot));

            s[c] = s[c].wrapping_add(s[d]);
            s[b] ^= s[c];
            let rot2 = ROTATE[(i + 1) % 4];
            s[b] = (s[b] << rot2) | (s[b] >> (32 - rot2));
        }
    }

    /// One double round of ChaCha20 (columns + diagonals).
    #[inline]
    fn double_round(&mut self) {
        Self::quarter_round(&mut self.state, 0, 4, 8, 12);
        Self::quarter_round(&mut self.state, 1, 5, 9, 13);
        Self::quarter_round(&mut self.state, 2, 6, 10, 14);
        Self::quarter_round(&mut self.state, 3, 7, 11, 15);
        Self::quarter_round(&mut self.state, 0, 5, 10, 15);
        Self::quarter_round(&mut self.state, 1, 6, 11, 12);
        Self::quarter_round(&mut self.state, 2, 7, 8, 13);
        Self::quarter_round(&mut self.state, 3, 4, 9, 14);
    }

    /// Generate the next u32 output from a fresh ChaCha block.
    pub fn next_u32(&mut self) -> u32 {
        self.double_round();
        let v = self.state[0];
        // Increment counter in state for subsequent blocks
        self.state[8] = self.state[8].wrapping_add(1);
        v
    }

    /// Generate an f32 in [0, 1).
    pub fn gen_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / (1 << 24) as f32)
    }

    /// Box-Muller transform for standard normal distribution.
    pub fn gen_normal(&mut self) -> f64 {
        let u1 = self.gen_f64();
        let u2 = self.gen_f64();
        -2.0 * u1.ln().sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    fn gen_f64(&mut self) -> f64 {
        let lo = self.next_u32();
        let hi = self.next_u32();
        ((lo as u64) << 32 | hi as u64) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}
