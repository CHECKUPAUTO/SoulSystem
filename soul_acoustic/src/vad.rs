//! Detection d'activite vocale (VAD) sur PCM brut i16 — gate cheap -> modele cher.
//!
//! Contrairement a un "embedding" (le RMS par bloc n'a AUCUN contenu semantique),
//! ce module repond a UNE question pas chere : "y a-t-il de la voix dans cette
//! trame ?", pour decider s'il faut invoquer le STT / le modele lourd.
//!
//! Zero allocation : `process_frame` ne fait que lire la trame fournie ; aucune
//! Vec interne, aucun buffer grandissant. L'appelant decoupe le flux en trames.

/// RMS de la trame en amplitude normalisee [0, 1] (16-bit -> /32768).
pub fn rms_norm(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let mut acc = 0.0f64;
    for &s in frame {
        let x = s as f64 / 32768.0;
        acc += x * x;
    }
    (acc / frame.len() as f64).sqrt() as f32
}

/// Taux de passage par zero (fraction de changements de signe), dans [0, 1].
/// Voix voisee : ZCR modere ; bruit haute-frequence : ZCR eleve ; silence : bas.
pub fn zero_crossing_rate(frame: &[i16]) -> f32 {
    if frame.len() < 2 {
        return 0.0;
    }
    let mut crossings = 0usize;
    for w in frame.windows(2) {
        if (w[0] >= 0) != (w[1] >= 0) {
            crossings += 1;
        }
    }
    crossings as f32 / (frame.len() - 1) as f32
}

/// Gate VAD a energie adaptative (+ bande ZCR optionnelle) avec hangover.
/// Stateful : suit un plancher de bruit (EMA) et un compteur de maintien.
pub struct VadGate {
    energy_factor: f32, // voix si rms > plancher_bruit * energy_factor
    min_rms: f32,       // plancher absolu (anti-declenchement sur quasi-silence)
    zcr_min: f32,
    zcr_max: f32,
    noise_adapt: f32, // coefficient EMA du plancher de bruit (0..1)
    hangover: u32,    // trames de maintien apres la derniere trame voisee
    noise_floor: f32,
    hangover_counter: u32,
}

impl VadGate {
    /// Defauts raisonnables : voix = 3x le bruit, plancher ~-40 dBFS, ZCR non
    /// contraignant (l'energie domine), EMA douce, hangover de 5 trames.
    pub fn new() -> Self {
        Self {
            energy_factor: 3.0,
            min_rms: 0.01,
            zcr_min: 0.0,
            zcr_max: 1.0,
            noise_adapt: 0.05,
            hangover: 5,
            noise_floor: 0.0,
            hangover_counter: 0,
        }
    }

    pub fn with_energy_factor(mut self, f: f32) -> Self {
        self.energy_factor = f;
        self
    }
    pub fn with_min_rms(mut self, m: f32) -> Self {
        self.min_rms = m;
        self
    }
    pub fn with_zcr_band(mut self, lo: f32, hi: f32) -> Self {
        self.zcr_min = lo;
        self.zcr_max = hi;
        self
    }
    pub fn with_noise_adapt(mut self, a: f32) -> Self {
        self.noise_adapt = a;
        self
    }
    pub fn with_hangover(mut self, h: u32) -> Self {
        self.hangover = h;
        self
    }

    /// Plancher de bruit courant (EMA du RMS des trames non voisees).
    #[inline]
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    /// Reinitialise l'etat (plancher + hangover) pour un nouveau flux.
    pub fn reset(&mut self) {
        self.noise_floor = 0.0;
        self.hangover_counter = 0;
    }

    /// Decide si la trame est "active" (voix detectee ou maintien hangover).
    /// Zero allocation. Adapte le plancher de bruit sur les trames non voisees.
    pub fn process_frame(&mut self, frame: &[i16]) -> bool {
        let rms = rms_norm(frame);
        let zcr = zero_crossing_rate(frame);
        let threshold = self.min_rms.max(self.noise_floor * self.energy_factor);
        let voiced = rms > threshold && zcr >= self.zcr_min && zcr <= self.zcr_max;

        if voiced {
            self.hangover_counter = self.hangover;
            return true;
        }
        // Trame non voisee -> echantillon de bruit : adaptation EMA du plancher.
        self.noise_floor =
            self.noise_floor * (1.0 - self.noise_adapt) + rms * self.noise_adapt;
        if self.hangover_counter > 0 {
            self.hangover_counter -= 1;
            return true; // maintien : evite de couper une fin de mot
        }
        false
    }
}

impl Default for VadGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms_norm(&[0i16; 512]), 0.0);
        assert_eq!(rms_norm(&[]), 0.0);
    }

    #[test]
    fn rms_of_constant_amplitude() {
        let r = rms_norm(&[16384i16; 100]); // 0.5 plein-echelle
        assert!((r - 0.5).abs() < 1e-4, "rms={r}");
    }

    #[test]
    fn zcr_alternating_is_one_constant_is_zero() {
        let alt = [1000i16, -1000, 1000, -1000, 1000, -1000];
        assert!((zero_crossing_rate(&alt) - 1.0).abs() < 1e-6);
        assert_eq!(zero_crossing_rate(&[500i16; 64]), 0.0);
        assert_eq!(zero_crossing_rate(&[7i16]), 0.0); // trop court
    }

    #[test]
    fn detects_loud_after_silence() {
        let mut g = VadGate::new();
        let silence = [0i16; 256];
        for _ in 0..10 {
            assert!(!g.process_frame(&silence), "silence -> non voise");
        }
        let loud = [16384i16; 256]; // rms 0.5
        assert!(g.process_frame(&loud), "trame forte -> voise");
    }

    #[test]
    fn hangover_holds_then_releases() {
        let mut g = VadGate::new().with_hangover(2).with_min_rms(0.05);
        let loud = [16384i16; 256];
        let quiet = [200i16; 256]; // rms ~0.006 < min_rms -> non voise
        assert!(g.process_frame(&loud), "voise");
        assert!(g.process_frame(&quiet), "hangover 1");
        assert!(g.process_frame(&quiet), "hangover 2");
        assert!(!g.process_frame(&quiet), "hangover epuise -> non voise");
    }

    #[test]
    fn noise_floor_adapts_upward() {
        let mut g = VadGate::new().with_min_rms(0.05).with_noise_adapt(0.5);
        assert_eq!(g.noise_floor(), 0.0);
        let noise = [655i16; 256]; // 655/32768 ~ 0.0200, < min_rms -> non voise
        for _ in 0..20 {
            assert!(!g.process_frame(&noise));
        }
        let nf = g.noise_floor();
        assert!((nf - 0.0200).abs() < 1e-3, "plancher ~0.02 attendu, nf={nf}");
        assert!(nf > 0.0);
    }

    #[test]
    fn empty_frame_is_inactive() {
        let mut g = VadGate::new();
        assert!(!g.process_frame(&[]));
    }

    #[test]
    fn adapted_floor_can_suppress_a_signal_that_was_voiced_when_quiet() {
        let probe = [393i16; 256]; // rms ~0.012
        let quiet_noise = [164i16; 256]; // rms ~0.005 (< min_rms) -> non voise, adapte

        // (a) plancher a 0 : probe est voise (0.012 > min_rms 0.01)
        let mut g1 = VadGate::new()
            .with_min_rms(0.01)
            .with_energy_factor(3.0)
            .with_noise_adapt(0.5);
        assert!(g1.process_frame(&probe), "plancher 0 -> probe voise");

        // (b) plancher appris ~0.005 -> seuil ~0.015 : meme signal rejete
        let mut g2 = VadGate::new()
            .with_min_rms(0.01)
            .with_energy_factor(3.0)
            .with_noise_adapt(0.5);
        for _ in 0..30 {
            g2.process_frame(&quiet_noise);
        }
        assert!(g2.noise_floor() > 0.004, "plancher appris, nf={}", g2.noise_floor());
        assert!(!g2.process_frame(&probe), "plancher eleve -> meme signal rejete");
    }
}

/// Segment voise : intervalle d'echantillons [start, end) dans le buffer PCM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicedSegment {
    pub start: usize,
    pub end: usize,
}

impl VadGate {
    /// Segmente un buffer PCM en regions voisees : decoupe en trames de
    /// `frame_len` echantillons, agrege les trames actives consecutives (le
    /// hangover prolonge naturellement chaque segment). La trame partielle
    /// finale (< frame_len) est ignoree. Le traitement par trame reste
    /// zero-alloc ; seul le Vec de sortie alloue.
    pub fn segment(&mut self, pcm: &[i16], frame_len: usize) -> Vec<VoicedSegment> {
        assert!(frame_len > 0, "frame_len doit etre > 0");
        let n_frames = pcm.len() / frame_len;
        let mut segments = Vec::new();
        let mut open: Option<usize> = None;
        for f in 0..n_frames {
            let start = f * frame_len;
            let active = self.process_frame(&pcm[start..start + frame_len]);
            match (active, open) {
                (true, None) => open = Some(start),
                (false, Some(s)) => {
                    segments.push(VoicedSegment { start: s, end: start });
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(s) = open {
            segments.push(VoicedSegment { start: s, end: n_frames * frame_len });
        }
        segments
    }
}

#[cfg(test)]
mod segment_tests {
    use super::*;

    const FL: usize = 4;

    fn build(frames: &[&[i16; FL]]) -> Vec<i16> {
        let mut v = Vec::new();
        for &f in frames {
            v.extend_from_slice(f);
        }
        v
    }

    #[test]
    fn all_silence_no_segments() {
        let mut g = VadGate::new().with_hangover(0);
        assert!(g.segment(&[0i16; 32], FL).is_empty());
    }

    #[test]
    fn single_voiced_region() {
        let mut g = VadGate::new().with_hangover(0);
        let s = [0i16; FL];
        let loud = [16384i16; FL];
        let pcm = build(&[&s, &s, &s, &loud, &loud, &s, &s, &s]); // silence x3, voix x2, silence x3
        assert_eq!(g.segment(&pcm, FL), vec![VoicedSegment { start: 12, end: 20 }]);
    }

    #[test]
    fn hangover_extends_segment() {
        let mut g = VadGate::new().with_hangover(2);
        let s = [0i16; FL];
        let loud = [16384i16; FL];
        let pcm = build(&[&s, &s, &s, &loud, &loud, &s, &s, &s]);
        // voix 12..20 ; hangover 2 trames -> +8 echantillons -> fin a 28
        assert_eq!(g.segment(&pcm, FL), vec![VoicedSegment { start: 12, end: 28 }]);
    }

    #[test]
    fn two_voiced_regions() {
        let mut g = VadGate::new().with_hangover(0);
        let s = [0i16; FL];
        let loud = [16384i16; FL];
        let pcm = build(&[&s, &loud, &s, &loud, &s]);
        assert_eq!(
            g.segment(&pcm, FL),
            vec![
                VoicedSegment { start: 4, end: 8 },
                VoicedSegment { start: 12, end: 16 },
            ]
        );
    }

    #[test]
    fn voiced_to_end_closes_segment() {
        let mut g = VadGate::new().with_hangover(0);
        let s = [0i16; FL];
        let loud = [16384i16; FL];
        let pcm = build(&[&s, &s, &loud, &loud]); // voix jusqu'a la fin
        assert_eq!(g.segment(&pcm, FL), vec![VoicedSegment { start: 8, end: 16 }]);
    }

    #[test]
    #[should_panic(expected = "frame_len")]
    fn frame_len_zero_panics() {
        let mut g = VadGate::new();
        g.segment(&[0i16; 16], 0);
    }
}
