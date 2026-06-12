use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum CritiqueError {
    #[error("No input to critique")]
    Empty,
    #[error("Critique limit exceeded: {0}")]
    LimitExceeded(usize),
}

pub type Result<T> = std::result::Result<T, CritiqueError>;

// ─── Quality Dimensions ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum QualityDimension {
    Correctness,
    Efficiency,
    Safety,
    Completeness,
    Clarity,
    Robustness,
}

impl QualityDimension {
    pub fn all() -> Vec<QualityDimension> {
        vec![
            Self::Correctness,
            Self::Efficiency,
            Self::Safety,
            Self::Completeness,
            Self::Clarity,
            Self::Robustness,
        ]
    }

    pub fn weight(&self) -> f64 {
        match self {
            Self::Correctness => 1.0,
            Self::Safety => 0.9,
            Self::Robustness => 0.8,
            Self::Completeness => 0.7,
            Self::Efficiency => 0.6,
            Self::Clarity => 0.5,
        }
    }
}

// ─── Quality Score ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    pub dimension: QualityDimension,
    pub score: f64, // 0.0 - 10.0
    pub reasoning: String,
    pub suggestions: Vec<String>,
}

impl QualityScore {
    pub fn new(dimension: QualityDimension, score: f64, reasoning: &str) -> Self {
        Self {
            dimension,
            score: score.clamp(0.0, 10.0),
            reasoning: reasoning.into(),
            suggestions: Vec::new(),
        }
    }

    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    pub fn is_passing(&self, threshold: f64) -> bool {
        self.score >= threshold
    }

    pub fn weighted_score(&self) -> f64 {
        self.score * self.dimension.weight()
    }
}

// ─── Critique Result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CritiqueResult {
    pub id: String,
    pub task_description: String,
    pub output: String,
    pub scores: Vec<QualityScore>,
    pub overall_score: f64,
    pub passed: bool,
    pub feedback: String,
    pub iteration: usize,
    pub timestamp: DateTime<Utc>,
}

impl CritiqueResult {
    pub fn calculate_overall(&mut self) {
        if self.scores.is_empty() {
            self.overall_score = 0.0;
            return;
        }

        let total_weight: f64 = self.scores.iter().map(|s| s.dimension.weight()).sum();
        let weighted_sum: f64 = self.scores.iter().map(|s| s.weighted_score()).sum();
        self.overall_score = if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        };
    }

    pub fn failed_dimensions(&self, threshold: f64) -> Vec<&QualityScore> {
        self.scores
            .iter()
            .filter(|s| !s.is_passing(threshold))
            .collect()
    }

    pub fn to_feedback(&self) -> String {
        let mut parts = vec![format!(
            "Overall: {:.1}/10 ({})",
            self.overall_score,
            if self.passed { "PASS" } else { "FAIL" }
        )];

        for score in &self.scores {
            let status = if score.is_passing(7.0) { "✓" } else { "✗" };
            parts.push(format!(
                "  {} {:?}: {:.1}/10 — {}",
                status, score.dimension, score.score, score.reasoning
            ));
            for sug in &score.suggestions {
                parts.push(format!("    → {sug}"));
            }
        }

        if !self.feedback.is_empty() {
            parts.push(format!("\nFeedback: {}", self.feedback));
        }

        parts.join("\n")
    }
}

// ─── Self-Critique Engine ────────────────────────────────────────────────────

pub struct CritiqueEngine {
    threshold: f64,
    max_iterations: usize,
}

impl CritiqueEngine {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            max_iterations: 3,
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    pub fn critique(
        &self,
        task_description: &str,
        output: &str,
        scores: Vec<QualityScore>,
        iteration: usize,
    ) -> CritiqueResult {
        let mut result = CritiqueResult {
            id: Uuid::new_v4().to_string(),
            task_description: task_description.into(),
            output: output.into(),
            scores,
            overall_score: 0.0,
            passed: false,
            feedback: String::new(),
            iteration,
            timestamp: Utc::now(),
        };

        result.calculate_overall();
        result.passed = result.overall_score >= self.threshold;

        if !result.passed {
            let failed: Vec<String> = result
                .failed_dimensions(self.threshold)
                .iter()
                .map(|s| format!("{:?}: {:.1}/10 — {}", s.dimension, s.score, s.reasoning))
                .collect();
            result.feedback = format!(
                "Failed dimensions (threshold {}):\n{}",
                self.threshold,
                failed.join("\n")
            );
        }

        result
    }

    pub fn should_retry(&self, result: &CritiqueResult) -> bool {
        !result.passed && result.iteration < self.max_iterations
    }

    pub fn generate_retry_prompt(&self, result: &CritiqueResult) -> String {
        let mut prompt = String::from("Your previous output did not meet quality standards.\n\n");

        prompt.push_str("Failed dimensions:\n");
        for score in result.failed_dimensions(self.threshold) {
            prompt.push_str(&format!(
                "- {:?}: {:.1}/10 — {}\n",
                score.dimension, score.score, score.reasoning
            ));
            for sug in &score.suggestions {
                prompt.push_str(&format!("  → {sug}\n"));
            }
        }

        prompt.push_str("\nPlease revise your output addressing these issues.");
        prompt
    }
}

// ─── Reflexion Loop (self-critique + revise) ─────────────────────────────────

pub struct ReflexionLoop {
    engine: CritiqueEngine,
}

impl ReflexionLoop {
    pub fn new(threshold: f64, max_rounds: usize) -> Self {
        Self {
            engine: CritiqueEngine::new(threshold).with_max_iterations(max_rounds),
        }
    }

    pub fn evaluate_round(
        &self,
        task: &str,
        output: &str,
        scores: Vec<QualityScore>,
        round: usize,
    ) -> (CritiqueResult, bool) {
        let result = self.engine.critique(task, output, scores, round);
        let should_continue = self.engine.should_retry(&result);
        (result, should_continue)
    }

    pub fn final_report(&self, history: &[CritiqueResult]) -> String {
        if history.is_empty() {
            return "No critique history.".into();
        }

        let mut report = format!("Reflexion Report ({} rounds)\n", history.len());
        report.push_str(&"=".repeat(50));
        report.push('\n');

        for (i, result) in history.iter().enumerate() {
            report.push_str(&format!(
                "\nRound {}: {:.1}/10 ({})\n",
                i + 1,
                result.overall_score,
                if result.passed { "PASS" } else { "FAIL" }
            ));
            for score in &result.scores {
                report.push_str(&format!("  {:?}: {:.1}\n", score.dimension, score.score));
            }
        }

        if let Some(last) = history.last() {
            report.push_str(&format!(
                "\nFinal: {:.1}/10 ({})\n",
                last.overall_score,
                if last.passed { "PASS" } else { "FAIL" }
            ));
        }

        report
    }
}

// ─── Convenience: Quick critique with auto-scoring ───────────────────────────

pub fn quick_critique(task: &str, output: &str) -> CritiqueResult {
    let mut scores = Vec::new();

    // Auto-score based on simple heuristics
    let correctness = if output.contains("error") || output.contains("fail") {
        4.0
    } else if output.len() > 50 {
        7.0
    } else {
        5.0
    };

    let completeness = if output.len() > 200 {
        8.0
    } else if output.len() > 100 {
        6.0
    } else {
        4.0
    };

    let clarity = if output.lines().count() > 1 {
        7.0
    } else if output.len() > 20 {
        6.0
    } else {
        3.0
    };

    scores.push(QualityScore::new(
        QualityDimension::Correctness,
        correctness,
        "Auto-scored",
    ));
    scores.push(QualityScore::new(
        QualityDimension::Completeness,
        completeness,
        "Auto-scored",
    ));
    scores.push(QualityScore::new(
        QualityDimension::Clarity,
        clarity,
        "Auto-scored",
    ));
    scores.push(QualityScore::new(
        QualityDimension::Efficiency,
        7.0,
        "Auto-scored",
    ));
    scores.push(QualityScore::new(
        QualityDimension::Safety,
        8.0,
        "Auto-scored",
    ));
    scores.push(QualityScore::new(
        QualityDimension::Robustness,
        6.0,
        "Auto-scored",
    ));

    let engine = CritiqueEngine::new(7.0);
    engine.critique(task, output, scores, 1)
}

// ─── LLM-based Critique ─────────────────────────────────────────────────────

/// Evaluate output using an LLM (Ollama). Falls back to quick_critique on error.
pub async fn llm_critique_with_fallback(task: &str, output: &str, model: &str) -> CritiqueResult {
    match llm_critique(task, output, model).await {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("LLM critique failed, falling back to heuristic: {e}");
            quick_critique(task, output)
        }
    }
}

/// Evaluate output using an LLM (Ollama). Returns a CritiqueResult with scores.
pub async fn llm_critique(
    task: &str,
    output: &str,
    model: &str,
) -> std::result::Result<CritiqueResult, String> {
    let prompt = format!(
        r#"You are a code quality reviewer. Evaluate this output on 6 dimensions (0-10).

TASK: {task}

OUTPUT:
{output}

Rate each dimension with a score (0-10) and brief reasoning. Return ONLY valid JSON:
{{
  "correctness": {{"score": <0-10>, "reasoning": "..."}},
  "completeness": {{"score": <0-10>, "reasoning": "..."}},
  "clarity": {{"score": <0-10>, "reasoning": "..."}},
  "efficiency": {{"score": <0-10>, "reasoning": "..."}},
  "safety": {{"score": <0-10>, "reasoning": "..."}},
  "robustness": {{"score": <0-10>, "reasoning": "..."}}
}}"#
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("http://localhost:11434/api/generate")
        .json(&serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let response_text = body["response"].as_str().unwrap_or("");

    // Extract JSON from response (handle markdown code blocks)
    let json_str = if let Some(start) = response_text.find('{') {
        if let Some(end) = response_text.rfind('}') {
            &response_text[start..=end]
        } else {
            response_text
        }
    } else {
        response_text
    };

    let scores_map: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse LLM response as JSON: {e}"))?;

    let mut scores = Vec::new();
    let dimensions = [
        ("correctness", QualityDimension::Correctness),
        ("completeness", QualityDimension::Completeness),
        ("clarity", QualityDimension::Clarity),
        ("efficiency", QualityDimension::Efficiency),
        ("safety", QualityDimension::Safety),
        ("robustness", QualityDimension::Robustness),
    ];

    for (key, dimension) in &dimensions {
        if let Some(obj) = scores_map.get(key) {
            let score = obj["score"].as_f64().unwrap_or(5.0);
            let reasoning = obj["reasoning"]
                .as_str()
                .unwrap_or("LLM evaluated")
                .to_string();
            scores.push(QualityScore::new(*dimension, score, &reasoning));
        }
    }

    let engine = CritiqueEngine::new(7.0);
    Ok(engine.critique(task, output, scores, 1))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_score() {
        let s = QualityScore::new(QualityDimension::Correctness, 8.5, "looks good");
        assert!(s.is_passing(7.0));
        assert!(!s.is_passing(9.0));
    }

    #[test]
    fn test_weighted_score() {
        let s = QualityScore::new(QualityDimension::Correctness, 10.0, "");
        assert_eq!(s.weighted_score(), 10.0); // weight 1.0

        let s2 = QualityScore::new(QualityDimension::Clarity, 10.0, "");
        assert_eq!(s2.weighted_score(), 5.0); // weight 0.5
    }

    #[test]
    fn test_critique_pass() {
        let engine = CritiqueEngine::new(5.0);
        let scores = vec![
            QualityScore::new(QualityDimension::Correctness, 8.0, "ok"),
            QualityScore::new(QualityDimension::Efficiency, 7.0, "ok"),
        ];
        let result = engine.critique("task", "output", scores, 1);
        assert!(result.passed);
    }

    #[test]
    fn test_critique_fail() {
        let engine = CritiqueEngine::new(8.0);
        let scores = vec![
            QualityScore::new(QualityDimension::Correctness, 5.0, "low"),
            QualityScore::new(QualityDimension::Efficiency, 4.0, "low"),
        ];
        let result = engine.critique("task", "output", scores, 1);
        assert!(!result.passed);
        assert!(!result.feedback.is_empty());
    }

    #[test]
    fn test_should_retry() {
        let engine = CritiqueEngine::new(8.0).with_max_iterations(3);
        let scores = vec![QualityScore::new(QualityDimension::Correctness, 3.0, "bad")];

        let r1 = engine.critique("task", "output", scores.clone(), 1);
        assert!(engine.should_retry(&r1));

        let r2 = engine.critique("task", "output", scores.clone(), 2);
        assert!(engine.should_retry(&r2));

        let r3 = engine.critique("task", "output", scores, 3);
        assert!(!engine.should_retry(&r3));
    }

    #[test]
    fn test_retry_prompt() {
        let engine = CritiqueEngine::new(8.0);
        let scores = vec![QualityScore::new(QualityDimension::Safety, 3.0, "insecure")
            .with_suggestion("use HTTPS")];

        let result = engine.critique("task", "output", scores, 1);
        let prompt = engine.generate_retry_prompt(&result);
        assert!(prompt.contains("Safety"));
        assert!(prompt.contains("use HTTPS"));
    }

    #[test]
    fn test_reflexion_loop() {
        let loop_engine = ReflexionLoop::new(8.0, 3);
        let scores = vec![QualityScore::new(QualityDimension::Correctness, 5.0, "low")];

        let (result, should_continue) = loop_engine.evaluate_round("task", "output", scores, 1);
        assert!(!result.passed);
        assert!(should_continue);
    }

    #[test]
    fn test_quick_critique() {
        let result = quick_critique("test task", "This is a test output with some content.");
        assert!(result.overall_score > 0.0);
        assert_eq!(result.scores.len(), 6);
    }

    #[test]
    fn test_critique_result_feedback() {
        let engine = CritiqueEngine::new(8.0);
        let scores = vec![
            QualityScore::new(QualityDimension::Correctness, 3.0, "wrong"),
            QualityScore::new(QualityDimension::Safety, 4.0, "unsafe"),
        ];
        let result = engine.critique("task", "output", scores, 1);
        let feedback = result.to_feedback();
        assert!(feedback.contains("FAIL"));
        assert!(feedback.contains("Correctness"));
    }

    #[test]
    fn test_dimension_all() {
        assert_eq!(QualityDimension::all().len(), 6);
    }

    #[test]
    fn test_dimension_weights() {
        assert!(QualityDimension::Correctness.weight() > QualityDimension::Clarity.weight());
    }
}
