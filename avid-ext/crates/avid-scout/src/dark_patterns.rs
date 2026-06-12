#![allow(
    clippy::cast_precision_loss,
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

/// Detected dark patterns.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DarkPatterns {
    pub urgency_clauses: Vec<String>,
    pub scarcity_clauses: Vec<String>,
    pub forced_continuity: bool,
    pub hidden_costs: bool,
    pub confirmshaming: bool,
    pub score: u8,
}

const URGENCY: &[&str] = &[
    "limited time",
    "hurry",
    "act now",
    "expires soon",
    "last chance",
    "only today",
    "ends tonight",
    "don't miss out",
    "final hours",
];

const SCARCITY: &[&str] = &[
    "only left",
    "almost gone",
    "limited stock",
    "selling fast",
    "only a few",
    "low stock",
    "out of stock soon",
];

const FORCED_CONTINUITY_PATTERNS: &[&str] =
    &["free trial", "start free", "subscribe now", "join now"];

const HIDDEN_COST_PATTERNS: &[&str] = &[
    "+ tax",
    "+ shipping",
    "fees may apply",
    "additional charges",
];

const CONFIRMSHAMING: &[&str] = &[
    "no thanks",
    "i don't want",
    "i'm not interested",
    "miss out",
    "not now",
    "no, i'll pass",
];

/// Detect dark UX patterns in page text and forms.
#[must_use]
pub fn detect_dark_patterns(text: &str, _forms: &[crate::forms::FormInfo]) -> DarkPatterns {
    let lower = text.to_lowercase();
    let mut urgency = Vec::new();
    let mut scarcity = Vec::new();

    for &pattern in URGENCY {
        if lower.contains(pattern) {
            urgency.push(pattern.to_string());
        }
    }
    for &pattern in SCARCITY {
        if lower.contains(pattern) {
            scarcity.push(pattern.to_string());
        }
    }

    let forced = FORCED_CONTINUITY_PATTERNS.iter().any(|p| lower.contains(p));
    let hidden = HIDDEN_COST_PATTERNS.iter().any(|p| lower.contains(p));
    let confirm = CONFIRMSHAMING.iter().any(|p| lower.contains(p));

    let score = (urgency.len() + scarcity.len()) as u8 * 10
        + if forced { 15 } else { 0 }
        + if hidden { 15 } else { 0 }
        + if confirm { 15 } else { 0 };

    DarkPatterns {
        urgency_clauses: urgency,
        scarcity_clauses: scarcity,
        forced_continuity: forced,
        hidden_costs: hidden,
        confirmshaming: confirm,
        score: score.min(100),
    }
}
