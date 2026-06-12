//! Extracteurs de fichiers média — portés depuis cobalt.tools
//!
//! Chaque module extrait les informations de téléchargement depuis
//! une plateforme spécifique. Les extracteurs JS originaux sont
//! conservés dans le répertoire services/ pour référence.

/// YouTube — vidéos, shorts, HLS/DASH
pub mod youtube;

/// Twitter/X — vidéos, gifs, carrousels
pub mod twitter;

/// Instagram — reels, stories, posts, carrousels
pub mod instagram;

/// TikTok — vidéos, carrousels, audio
pub mod tiktok;

/// Reddit — vidéos hébergées
pub mod reddit;

/// Détection de service et routage
pub mod router;

pub use router::detect_and_extract;
