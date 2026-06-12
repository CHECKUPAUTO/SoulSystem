//! Reporter : construit `ReportStats` à partir d'une liste de synergies,
//! et expose des helpers de rapport.

use crate::types::{ReportStats, ScanReport, Synergy};
use std::collections::HashMap;

pub fn build_stats(synergies: &[Synergy]) -> ReportStats {
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut by_priority: HashMap<u8, usize> = HashMap::new();
    let mut by_status: HashMap<String, usize> = HashMap::new();
    let mut total_score = 0.0f32;
    for s in synergies {
        *by_type.entry(s.synergy_type.clone()).or_default() += 1;
        *by_priority.entry(s.priority).or_default() += 1;
        *by_status.entry(format!("{:?}", s.status)).or_default() += 1;
        total_score += s.adjusted_score;
    }
    let mean = if synergies.is_empty() {
        0.0
    } else {
        total_score / synergies.len() as f32
    };
    ReportStats {
        total: synergies.len(),
        by_type,
        by_priority,
        by_status,
        mean_adjusted_score: mean,
    }
}

pub fn summary_string(r: &ScanReport) -> String {
    let mut top: Vec<&Synergy> = r.synergies.iter().collect();
    top.sort_by(|a, b| {
        b.adjusted_score
            .partial_cmp(&a.adjusted_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let head: Vec<String> = top
        .iter()
        .take(5)
        .map(|s| {
            format!(
                "  [{:.2}] {} → {} ({})",
                s.adjusted_score, s.source_project, s.target_project, s.synergy_type
            )
        })
        .collect();
    format!(
        "Scan terminé : {} projets, {} synergies en {} ms, score moyen {:.2}\nTop 5 :\n{}",
        r.projects.len(),
        r.synergies.len(),
        r.elapsed_ms,
        r.stats.mean_adjusted_score,
        head.join("\n")
    )
}
