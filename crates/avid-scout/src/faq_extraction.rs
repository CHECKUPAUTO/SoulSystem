#![allow(
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::bool_to_int_with_if,
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::use_self,
    clippy::redundant_clone,
    clippy::wildcard_imports,
    clippy::option_if_let_else,
    clippy::manual_split_once,
    clippy::match_wildcard_for_single_variants,
    clippy::single_char_pattern,
    clippy::range_plus_one,
    clippy::unnecessary_map_or,
    clippy::manual_pattern_char_comparison,
    clippy::suboptimal_flops,
    clippy::needless_collect,
    clippy::inefficient_to_string
)]

/// FAQ item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FaqItem {
    pub question: String,
    pub answer: String,
}

/// Extract FAQ items from JSON-LD or heuristics.
#[must_use]
pub fn extract_faq(html: &str, text: &str) -> Vec<FaqItem> {
    let mut faqs = Vec::new();
    let lower = html.to_lowercase();

    // JSON-LD FAQPage
    if lower.contains("\"@type\":\"faqpage\"") || lower.contains("\"@type\": \"faqpage\"") {
        for block in text.split("?").collect::<Vec<_>>().chunks(2) {
            if block.len() == 2 {
                let q = block[0].trim().to_string();
                let a = block[1].trim().to_string();
                if !q.is_empty() && !a.is_empty() && q.len() < 200 {
                    faqs.push(FaqItem {
                        question: q + "?",
                        answer: a,
                    });
                }
            }
        }
    }

    // Heuristic: look for Q: / A: patterns
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Q:") || trimmed.starts_with("Q. ") {
            let q = trimmed
                .trim_start_matches("Q:")
                .trim_start_matches("Q.")
                .trim()
                .to_string();
            if !q.is_empty() {
                faqs.push(FaqItem {
                    question: q,
                    answer: String::new(),
                });
            }
        } else if trimmed.starts_with("A:") || trimmed.starts_with("A. ") {
            if let Some(last) = faqs.last_mut() {
                last.answer = trimmed
                    .trim_start_matches("A:")
                    .trim_start_matches("A.")
                    .trim()
                    .to_string();
            }
        }
    }

    faqs.into_iter()
        .filter(|f| !f.question.is_empty())
        .collect()
}
