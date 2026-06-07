//! Injecteur de fautes deterministe (chaos engineering) : perturbe un buffer
//! avec une probabilite donnee, de maniere reproductible (PRNG a graine).

pub struct ChaosMonkey {
    fault_rate: f32,
    state: u64,
}

impl ChaosMonkey {
    pub fn new(seed: u64, fault_rate: f32) -> Self {
        Self {
            fault_rate: fault_rate.clamp(0.0, 1.0),
            state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed },
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    #[inline]
    fn next_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Perturbe chaque element avec proba `fault_rate` (+bruit borne dans
    /// [-magnitude, magnitude]). Renvoie le nombre de fautes injectees.
    pub fn perturb(&mut self, buf: &mut [f32], magnitude: f32) -> usize {
        let mut faults = 0;
        for v in buf.iter_mut() {
            if self.next_unit() < self.fault_rate {
                let noise = (self.next_unit() * 2.0 - 1.0) * magnitude;
                *v += noise;
                faults += 1;
            }
        }
        faults
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_zero_aucune_faute() {
        let mut m = ChaosMonkey::new(42, 0.0);
        let mut buf = vec![1.0f32; 1000];
        let before = buf.clone();
        assert_eq!(m.perturb(&mut buf, 10.0), 0);
        assert_eq!(buf, before);
        println!("PREUVE chaos rate=0 : 0 faute, buffer intact");
    }

    #[test]
    fn rate_un_perturbe_tout_borne() {
        let mut m = ChaosMonkey::new(42, 1.0);
        let mut buf = vec![1.0f32; 1000];
        assert_eq!(m.perturb(&mut buf, 0.5), 1000);
        for v in &buf {
            assert!((*v - 1.0).abs() <= 0.5 + 1e-6);
        }
        println!("PREUVE chaos rate=1 : 1000 fautes, perturbations bornees +-0.5");
    }

    #[test]
    fn deterministe_meme_graine() {
        let mut a = ChaosMonkey::new(7, 0.3);
        let mut b = ChaosMonkey::new(7, 0.3);
        let mut ba = vec![0.0f32; 500];
        let mut bb = vec![0.0f32; 500];
        let fa = a.perturb(&mut ba, 1.0);
        assert_eq!(fa, b.perturb(&mut bb, 1.0));
        assert_eq!(ba, bb);
        println!("PREUVE chaos deterministe : meme graine -> {} fautes identiques", fa);
    }
}
