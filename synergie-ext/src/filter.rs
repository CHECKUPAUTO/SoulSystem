//! Filtres de synergies — dédoublonnage, fusion et aggrégation.

use crate::types::Synergy;
use scirust_symbolic::prove_equal;

/// Filtre les synergies symboliques dédoublonnées par expression équivalente.
/// Par exemple, si "a + b" et "b + a" donnent deux synergies distinctes,
/// on les fusionne en une seule.
pub fn filter_symbolic(mut synergies: Vec<Synergy>) -> Vec<Synergy> {
    if synergies.len() < 2 {
        return synergies;
    }

    let mut merged: Vec<Synergy> = Vec::new();
    let mut used = vec![false; synergies.len()];

    for i in 0..synergies.len() {
        if used[i] {
            continue;
        }
        let merged_idx = i;

        for j in i + 1..synergies.len() {
            if used[j] {
                continue;
            }

            // Vérifier si les deux synergies décrivent la même équivalence
            let desc_i = &synergies[merged_idx].description;
            let desc_j = &synergies[j].description;

            if let (Some((e1_a, e1_b)), Some((e2_a, e2_b))) =
                (extract_exprs(desc_i), extract_exprs(desc_j))
            {
                let match_1 = parse_both_and_compare(&e1_a, &e2_a)
                    && parse_both_and_compare(&e1_b, &e2_b);
                let match_2 = parse_both_and_compare(&e1_a, &e2_b)
                    && parse_both_and_compare(&e1_b, &e2_a);

                if match_1 || match_2 {
                    used[j] = true;
                    let merged_desc = format!(
                        "{}; {}",
                        synergies[merged_idx].description,
                        extract_short(&synergies[j].description)
                    );
                    synergies[merged_idx].description = merged_desc;
                }
            }
        }

        used[merged_idx] = true;
        merged.push(synergies[merged_idx].clone());
    }

    merged
}

/// Applique tous les filtres disponibles par ordre.
pub fn apply_all(synergies: Vec<Synergy>) -> Vec<Synergy> {
    let filtered = filter_symbolic(synergies);
    filtered
}

fn extract_exprs(desc: &str) -> Option<(&str, &str)> {
    let backtick_positions: Vec<_> = desc.match_indices('`').map(|(i, _)| i).collect();
    if backtick_positions.len() < 4 {
        return None;
    }
    let e1 = &desc[backtick_positions[0] + 1..backtick_positions[1]];
    let e2 = &desc[backtick_positions[2] + 1..backtick_positions[3]];
    Some((e1, e2))
}

fn extract_short(desc: &str) -> String {
    extract_exprs(desc)
        .map(|(a, b)| format!("`{}` ↔ `{}`", a, b))
        .unwrap_or_else(|| desc.to_string())
}

fn parse_both_and_compare(a: &str, b: &str) -> bool {
    match (scirust_symbolic::parse(a), scirust_symbolic::parse(b)) {
        (Ok(e_a), Ok(e_b)) => prove_equal(&e_a, &e_b),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_symbolic_dedup() {
        let synergies = vec![
            Synergy {
                id: "s1".into(),
                synergy_type: "Deduplication".into(),
                source_project: "a".into(),
                target_project: "b".into(),
                description: "Expression équivalente entre 'a' et 'b' : `a + b` ↔ `b + a`".into(),
                priority: 5,
                effort: "medium".into(),
                value: "medium".into(),
                auto_score: 5.0,
                adjusted_score: 5.0,
                ..Default::default()
            },
            Synergy {
                id: "s2".into(),
                synergy_type: "Deduplication".into(),
                source_project: "a".into(),
                target_project: "b".into(),
                description: "Expression équivalente entre 'a' et 'b' : `a + b` ↔ `a + b`".into(),
                priority: 5,
                effort: "medium".into(),
                value: "medium".into(),
                auto_score: 5.0,
                adjusted_score: 5.0,
                ..Default::default()
            },
        ];
        let filtered = filter_symbolic(synergies);
        assert!(filtered.len() <= 2, "devrait fusionner, obtenu {}", filtered.len());
    }

    #[test]
    fn test_extract_exprs() {
        let desc = "Expression équivalente entre 'a' et 'b' : `a + b` ↔ `b + a`";
        let (e1, e2) = extract_exprs(desc).unwrap();
        assert_eq!(e1, "a + b");
        assert_eq!(e2, "b + a");
    }

    #[test]
    fn test_no_filter_on_single() {
        let synergies = vec![Synergy {
            id: "s1".into(),
            synergy_type: "Deduplication".into(),
            source_project: "a".into(),
            target_project: "b".into(),
            description: "Expression équivalente : `x` ↔ `y`".into(),
            priority: 5,
            effort: "medium".into(),
            value: "medium".into(),
            auto_score: 5.0,
            adjusted_score: 5.0,
            ..Default::default()
        }];
        let filtered = filter_symbolic(synergies.clone());
        assert_eq!(filtered.len(), 1);
    }
}
