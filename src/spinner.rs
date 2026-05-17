//! Spinner — Animation tournante pour les messages Telegram en streaming.
//!
//! Fournit une structure légère et clonable qui itère sur une liste de frames
//! pour afficher une animation de progression dans les messages.

/// Frame par défaut pour le spinner braille.
const DEFAULT_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Indicateur de progression animé (spinner).
///
/// Clone-safe, utilisable dans un contexte async. Itère cycliquement
/// sur une liste de frames Unicode (braille par défaut).
#[derive(Clone)]
pub struct Spinner {
    frames: Vec<String>,
    index: usize,
}

impl Spinner {
    /// Crée un nouveau spinner avec les frames braille par défaut.
    pub fn new() -> Self {
        Self {
            frames: DEFAULT_FRAMES.iter().map(|s| s.to_string()).collect(),
            index: 0,
        }
    }

    /// Crée un spinner avec des frames personnalisées.
    pub fn with_frames(frames: Vec<String>) -> Self {
        Self { frames, index: 0 }
    }

    /// Retourne la frame courante et avance à la suivante (cyclique).
    pub fn next_frame(&mut self) -> &str {
        let frame = &self.frames[self.index];
        self.index = (self.index + 1) % self.frames.len();
        frame
    }

    /// Réinitialise le spinner à sa première frame.
    pub fn reset(&mut self) {
        self.index = 0;
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Spinner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spinner")
            .field("index", &self.index)
            .field("current", &self.frames[self.index])
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_cycles_through_frames() {
        let mut spinner = Spinner::new();
        let first = spinner.next_frame().to_string();
        let second = spinner.next_frame().to_string();
        assert_ne!(first, second);
    }

    #[test]
    fn test_spinner_wraps_around() {
        let mut spinner = Spinner::with_frames(vec!["A".into(), "B".into(), "C".into()]);
        assert_eq!(spinner.next_frame(), "A");
        assert_eq!(spinner.next_frame(), "B");
        assert_eq!(spinner.next_frame(), "C");
        assert_eq!(spinner.next_frame(), "A");
    }

    #[test]
    fn test_spinner_reset() {
        let mut spinner = Spinner::with_frames(vec!["X".into(), "Y".into()]);
        assert_eq!(spinner.next_frame(), "X");
        assert_eq!(spinner.next_frame(), "Y");
        spinner.reset();
        assert_eq!(spinner.next_frame(), "X");
    }

    #[test]
    fn test_spinner_clone_independent() {
        let mut s1 = Spinner::with_frames(vec!["1".into(), "2".into()]);
        let mut s2 = s1.clone();
        s1.next_frame();
        assert_eq!(s2.next_frame(), "1");
    }
}
