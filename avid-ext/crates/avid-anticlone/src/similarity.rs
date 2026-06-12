use std::collections::BTreeMap;

use crate::depth::aggregate_depth;
use crate::Fingerprint;

/// Verdict for originality checking.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Verdict {
    Original,
    Clone,
}

/// Result of checking a candidate fingerprint against a corpus.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OriginalityReport {
    pub score: f32,
    pub verdict: Verdict,
    pub closest_id: Option<i64>,
}

/// Compute the structural similarity between two fingerprints.
///
/// Score = 0.4 * `weighted_jaccard(node_hist)` + 0.4 * `jaccard(edges)` + 0.2 * (1 - `normalized_euclidean(depth)`)
#[must_use]
pub fn similarity(a: &Fingerprint, b: &Fingerprint) -> f32 {
    let node_sim = weighted_jaccard(&a.node_hist, &b.node_hist);
    let edge_sim = jaccard_edges(&a.edges, &b.edges);
    let depth_sim = depth_similarity(
        &aggregate_depth(&a.depth_profiles),
        &aggregate_depth(&b.depth_profiles),
    );

    0.4_f32.mul_add(node_sim, 0.4_f32.mul_add(edge_sim, 0.2 * depth_sim))
}

/// Check a candidate fingerprint against a corpus of known fingerprints.
#[must_use]
pub fn check<'a, I: IntoIterator<Item = (i64, &'a Fingerprint)>>(
    candidate: &Fingerprint,
    corpus: I,
    threshold: f32,
) -> OriginalityReport {
    let mut max_score = 0.0_f32;
    let mut closest_id = None;

    for (id, fp) in corpus {
        let score = similarity(candidate, fp);
        if score > max_score {
            max_score = score;
            closest_id = Some(id);
        }
    }

    let verdict = if max_score > threshold {
        Verdict::Clone
    } else {
        Verdict::Original
    };

    OriginalityReport {
        score: max_score,
        verdict,
        closest_id,
    }
}

/// Weighted Jaccard similarity between two node histograms.
#[allow(clippy::cast_precision_loss)]
fn weighted_jaccard(a: &BTreeMap<String, u32>, b: &BTreeMap<String, u32>) -> f32 {
    let mut intersection = 0_u64;
    let mut union = 0_u64;

    let all_keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for key in all_keys {
        let ca = u64::from(*a.get(key).unwrap_or(&0));
        let cb = u64::from(*b.get(key).unwrap_or(&0));
        intersection += ca.min(cb);
        union += ca.max(cb);
    }

    if union == 0 {
        return 1.0;
    }
    intersection as f32 / union as f32
}

/// Jaccard similarity on the edge multiset.
#[allow(clippy::cast_precision_loss)]
fn jaccard_edges(a: &BTreeMap<(String, String), u32>, b: &BTreeMap<(String, String), u32>) -> f32 {
    let mut intersection = 0_u64;
    let mut union = 0_u64;

    let all_keys: std::collections::BTreeSet<&(String, String)> =
        a.keys().chain(b.keys()).collect();
    for key in all_keys {
        let ca = u64::from(*a.get(key).unwrap_or(&0));
        let cb = u64::from(*b.get(key).unwrap_or(&0));
        intersection += ca.min(cb);
        union += ca.max(cb);
    }

    if union == 0 {
        return 1.0;
    }
    intersection as f32 / union as f32
}

/// 1 - normalized Euclidean distance between depth profiles.
#[allow(clippy::cast_precision_loss)]
fn depth_similarity(a: &(u32, f32, f32), b: &(u32, f32, f32)) -> f32 {
    let da = [a.0 as f32, a.1, a.2];
    let db = [b.0 as f32, b.1, b.2];

    let squared: f32 = da.iter().zip(&db).map(|(x, y)| (x - y).powi(2)).sum();
    let dist = squared.sqrt();
    // Max possible distance for two (max_depth, mean, stdev) tuples — heuristically 1000
    let normalized = (dist / 1000.0).min(1.0);
    1.0 - normalized
}
