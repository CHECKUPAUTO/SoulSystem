//! EvoPulse — Pulsion d'Évolution Intrinsèque.
//!
//! Sous-système de motivation intrinsèque pour SoulLink. Convertit l'incertitude
//! de prédiction en inconfort BCI (CuriosityEngine) et la réduction d'erreur
//! en récompense (LearningHigh). Le tout est publié sur le bus Redis pour
//! communication inter-processus avec SoulSystem.

pub mod curiosity;
pub mod learning_high;
pub mod bus_publisher;
