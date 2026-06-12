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

use std::collections::HashMap;

/// Simple TF-IDF based text classifier.
pub struct TfidfClassifier {
    categories: HashMap<String, Vec<Vec<String>>>,
    idf: HashMap<String, f64>,
    doc_count: usize,
}

impl TfidfClassifier {
    pub fn new() -> Self {
        Self {
            categories: HashMap::new(),
            idf: HashMap::new(),
            doc_count: 0,
        }
    }

    pub fn train(&mut self, category: &str, documents: &[Vec<String>]) {
        self.categories
            .insert(category.to_string(), documents.to_vec());
        self.doc_count += documents.len();
        self.recompute_idf();
    }

    fn recompute_idf(&mut self) {
        let mut df = HashMap::<String, usize>::new();
        for docs in self.categories.values() {
            for doc in docs {
                let mut seen = Vec::new();
                for word in doc {
                    if !seen.contains(word) {
                        seen.push(word.clone());
                        *df.entry(word.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        for (word, count) in &df {
            let idf = ((self.doc_count as f64) / (*count as f64)).ln() + 1.0;
            self.idf.insert(word.clone(), idf);
        }
    }

    fn tf_idf(&self, doc: &[String]) -> HashMap<String, f64> {
        let mut tf = HashMap::<String, usize>::new();
        for word in doc {
            *tf.entry(word.clone()).or_insert(0) += 1;
        }
        let mut vec = HashMap::new();
        for (word, count) in &tf {
            let tf_val = *count as f64 / doc.len().max(1) as f64;
            let idf = self.idf.get(word).copied().unwrap_or(1.0);
            vec.insert(word.clone(), tf_val * idf);
        }
        vec
    }

    fn cosine_similarity(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for (word, val_a) in a {
            norm_a += val_a * val_a;
            if let Some(val_b) = b.get(word) {
                dot += val_a * val_b;
            }
        }
        for val in b.values() {
            norm_b += val * val;
        }
        if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a.sqrt() * norm_b.sqrt())
        } else {
            0.0
        }
    }

    /// Classify a document against trained categories.
    pub fn classify(&self, doc: &[String]) -> Option<(String, f64)> {
        let doc_vec = self.tf_idf(doc);
        let mut best = None;
        let mut best_score = -1.0;
        for (cat, docs) in &self.categories {
            let mut total_sim = 0.0;
            for train_doc in docs {
                let train_vec = self.tf_idf(train_doc);
                total_sim += Self::cosine_similarity(&doc_vec, &train_vec);
            }
            let avg = total_sim / docs.len().max(1) as f64;
            if avg > best_score {
                best_score = avg;
                best = Some(cat.clone());
            }
        }
        best.map(|b| (b, best_score))
    }
}

/// Tokenise text into lowercase words.
pub fn tokenise(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(String::from)
        .collect()
}
