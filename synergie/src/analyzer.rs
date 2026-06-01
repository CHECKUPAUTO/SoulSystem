//! Analyzer : helpers transverses pour les détecteurs.
//!
//! - `auto_score_from` : combine quantité d'evidence + qualité + plancher
//!   pour produire un `auto_score` cohérent entre détecteurs.
//! - `type_floor` : plancher de score par type de synergie.

pub fn auto_score_from(evidence_count: usize, mean_evidence_weight: f32, type_floor: f32) -> f32 {
    let n = evidence_count as f32;
    let qty = (n / (n + 4.0)).min(0.95);
    let q = mean_evidence_weight.clamp(0.0, 1.0);
    let base = type_floor.max(0.1);
    (base + (qty * 0.7 + q * 0.3)).min(1.5)
}

pub fn type_floor(ty: &str) -> f32 {
    match ty {
        "Fusion" => 0.6,
        "Deduplication" => 0.4,
        "ApiComplementarity" => 0.45,
        "SemanticOverlap" => 0.4,
        "LanguageBridge" => 0.5,
        "ServiceBus" => 0.45,
        "HiddenCoupling" => 0.35,
        "SharedDependency" => 0.30,
        "ConfigUnification" => 0.30,
        "DataPipeline" => 0.45,
        "DocCrossRef" => 0.20,
        _ => 0.3,
    }
}

/// Convertit un score normalisé `auto_score` ∈ ~[0, 1.5] en `priority` 1..10.
pub fn priority_from_score(s: f32) -> u8 {
    let p = (s * 6.0).round() as i32;
    p.clamp(1, 10) as u8
}

pub fn value_from_sources(n_projects: usize, mean_weight: f32) -> String {
    if n_projects >= 4 || mean_weight > 0.9 { "high".into() }
    else if n_projects >= 2 || mean_weight > 0.7 { "medium".into() }
    else { "low".into() }
}

pub fn effort_from_size(n_items: usize) -> String {
    match n_items {
        0..=3 => "easy".into(),
        4..=10 => "medium".into(),
        _ => "hard".into(),
    }
}

pub fn short_hash(s: &str) -> String {
    let h = blake3::hash(s.as_bytes());
    hex::encode(&h.as_bytes()[..3])
}
