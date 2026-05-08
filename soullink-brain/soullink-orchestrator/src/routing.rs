//! Routing logic — pure functions extracted from `main.rs` for benchability.
//!
//! Phase 4 moves `select_brains`, `attractor_score`, and `merge_concepts`
//! here so they can be exercised by criterion benches independent of the
//! axum handler layer. No behaviour change.

use std::collections::HashMap;

use ahash::AHashMap;
use serde_json::{json, Value};

/// Attractor quality score — preferred stability weight for turbulence-aware routing.
#[inline]
pub fn attractor_score(attractor: &str) -> f64 {
    match attractor {
        "StableOrbit"      => 1.0,
        "DeepBasin"        => 0.6,
        "Transient"        => 0.4,
        "Initializing"     => 0.3,
        "StrangeAttractor" => 0.1,
        _                  => 0.5,
    }
}

/// Select up to 3 brains whose speciality keywords appear in `query`,
/// always including "meta".
pub fn select_brains<I>(
    brain_specs: I,
    query: &str,
) -> Vec<String>
where
    I: IntoIterator<Item = (String, Vec<String>)>,
{
    let q = query.to_lowercase();
    let mut scores: Vec<(String, f64)> = brain_specs
        .into_iter()
        .map(|(key, spec)| {
            let score: f64 = spec
                .iter()
                .map(|kw| if q.contains(kw.as_str()) { 2.0 } else { 0.0 })
                .sum();
            let bonus = if key == "meta" { 0.5 } else { 0.0 };
            (key, score + bonus)
        })
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected: Vec<String> = scores.into_iter().take(3).map(|(k, _)| k).collect();
    if !selected.contains(&"meta".to_string()) {
        selected.push("meta".to_string());
    }
    selected.dedup();
    selected
}

/// Merge top_concepts from each brain's response, keeping the best-scoring
/// occurrence per concept. Phase 4 change: `AHashMap` instead of `HashMap`
/// for 2-3× speedup on small string keys (measured on T430).
pub fn merge_concepts(results: &HashMap<String, Value>) -> Vec<Value> {
    // Rough upper bound: 6 brains × 20 concepts each — pre-allocate to avoid rehashing.
    let mut all: AHashMap<String, (f64, f64, String)> = AHashMap::with_capacity(128);

    for (brain, r) in results {
        if let Some(concepts) = r.get("top_concepts").and_then(|v| v.as_array()) {
            for c in concepts {
                let concept = c.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                if concept.is_empty() { continue; }
                let mastery = c.get("mastery").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let score = c.get("score").and_then(|v| v.as_f64()).unwrap_or(mastery);

                let entry = all.entry(concept.to_string())
                    .or_insert_with(|| (0.0, 0.0, brain.clone()));
                if score > entry.0 {
                    *entry = (score, mastery, brain.clone());
                }
            }
        }
    }

    let mut sorted: Vec<_> = all.into_iter().collect();
    sorted.sort_by(|a, b| b.1.0.partial_cmp(&a.1.0).unwrap_or(std::cmp::Ordering::Equal));
    sorted
        .into_iter()
        .take(20)
        .map(|(concept, (score, mastery, brain))| {
            json!({
                "concept":      concept,
                "score":        score,
                "mastery":      mastery,
                "source_brain": brain,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_brain_specs() -> Vec<(String, Vec<String>)> {
        vec![
            ("science".into(),  vec!["physics", "math", "chemistry"].into_iter().map(String::from).collect()),
            ("mind".into(),     vec!["neuroscience", "language", "philosophy"].into_iter().map(String::from).collect()),
            ("engineer".into(), vec!["optimization", "logic", "algebra"].into_iter().map(String::from).collect()),
            ("crypto".into(),   vec!["trading", "defi", "markets"].into_iter().map(String::from).collect()),
            ("creative".into(), vec!["patterns", "art", "vision"].into_iter().map(String::from).collect()),
            ("meta".into(),     vec!["learning", "optimization"].into_iter().map(String::from).collect()),
        ]
    }

    #[test]
    fn attractor_score_known() {
        assert!(attractor_score("StableOrbit") > attractor_score("StrangeAttractor"));
        assert_eq!(attractor_score("unknown_attractor"), 0.5);
    }

    #[test]
    fn select_always_includes_meta() {
        let sel = select_brains(sample_brain_specs(), "what is quantum physics");
        assert!(sel.contains(&"meta".to_string()),
                "meta must always be in selection, got: {sel:?}");
    }

    #[test]
    fn select_picks_relevant_brains() {
        let sel = select_brains(sample_brain_specs(), "explain the physics of light");
        assert!(sel.contains(&"science".to_string()));
    }

    #[test]
    fn select_limits_primary_to_three() {
        // Query hits all 6 specialties
        let sel = select_brains(
            sample_brain_specs(),
            "physics neuroscience optimization trading art learning",
        );
        // 3 top + meta (already in top 3 here, so may be only 3 or 4 unique)
        assert!(sel.len() <= 4);
        assert!(sel.contains(&"meta".to_string()));
    }

    #[test]
    fn merge_deduplicates_keeping_max_score() {
        let mut results: HashMap<String, Value> = HashMap::new();
        results.insert("science".into(), json!({
            "top_concepts": [
                {"concept": "entropy", "score": 0.5, "mastery": 0.4},
                {"concept": "relativity", "score": 0.9, "mastery": 0.85},
            ]
        }));
        results.insert("mind".into(), json!({
            "top_concepts": [
                {"concept": "entropy", "score": 0.8, "mastery": 0.7},  // higher score
                {"concept": "memory", "score": 0.6, "mastery": 0.55},
            ]
        }));
        let merged = merge_concepts(&results);
        assert_eq!(merged.len(), 3);

        let entropy = merged.iter()
            .find(|c| c.get("concept").and_then(|v| v.as_str()) == Some("entropy"))
            .expect("entropy missing");
        assert_eq!(entropy.get("score").and_then(|v| v.as_f64()), Some(0.8));
        assert_eq!(entropy.get("source_brain").and_then(|v| v.as_str()), Some("mind"));
    }

    #[test]
    fn merge_caps_at_twenty() {
        let mut results: HashMap<String, Value> = HashMap::new();
        let concepts: Vec<Value> = (0..50)
            .map(|i| json!({"concept": format!("c{i}"), "score": i as f64 * 0.01, "mastery": 0.5}))
            .collect();
        results.insert("science".into(), json!({ "top_concepts": concepts }));
        let merged = merge_concepts(&results);
        assert_eq!(merged.len(), 20);
    }

    #[test]
    fn merge_handles_missing_fields() {
        let mut results: HashMap<String, Value> = HashMap::new();
        results.insert("a".into(), json!({
            "top_concepts": [
                {"concept": ""},                    // empty concept rejected
                {"concept": "valid", "mastery": 0.3}, // score falls back to mastery
            ]
        }));
        let merged = merge_concepts(&results);
        assert_eq!(merged.len(), 1);
        let c = &merged[0];
        assert_eq!(c.get("concept").and_then(|v| v.as_str()), Some("valid"));
        assert_eq!(c.get("score").and_then(|v| v.as_f64()), Some(0.3));
    }
}
