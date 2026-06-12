//! Automatic test generation and execution.
//! Uses LLM to generate inputs/expected outputs, runs them in the sandbox,
//! and computes a pass-rate score. Falls back to random-input crash tests
//! when the LLM is unavailable.

use crate::llm::LlmClient;

use crate::sandbox::{run_in_sandbox, SandboxConfig};
use anyhow::{Context, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TestGeneratorConfig {
    pub num_tests: usize,
    pub timeout_secs: u64,
    pub max_input_len: usize,
    pub fallback_random: bool,
}

impl Default for TestGeneratorConfig {
    fn default() -> Self {
        Self {
            num_tests: 5,
            timeout_secs: 10,
            max_input_len: 256,
            fallback_random: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub passed: usize,
    pub total: usize,
    pub score: f64,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TestCase {
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_output: Option<String>,
}

#[derive(Clone)]
pub struct TestGenerator {
    client: LlmClient,
    config: TestGeneratorConfig,
}

impl TestGenerator {
    pub fn new(client: LlmClient, config: &TestGeneratorConfig) -> Self {
        Self {
            client,
            config: config.clone(),
        }
    }

    /// Generate tests for the candidate code and run them in the sandbox.
    pub async fn generate_and_run(
        &self,
        code: &str,
        language: &str,
        work_dir: &Path,
        sandbox_config: &SandboxConfig,
    ) -> Result<TestResult> {
        let tests = if self.config.fallback_random {
            self.llm_generate_tests(code, language)
                .await
                .unwrap_or_else(|_| self.random_tests(code, language))
        } else {
            self.llm_generate_tests(code, language).await?
        };

        if tests.is_empty() {
            return Ok(TestResult {
                passed: 0,
                total: 0,
                score: 0.0,
                details: vec!["No tests generated".into()],
            });
        }

        let mut passed = 0usize;
        let mut details = Vec::new();
        for (i, test) in tests.iter().enumerate() {
            let result = self
                .run_single_test(code, language, work_dir, sandbox_config, test)
                .await;
            match result {
                Ok(true) => {
                    passed += 1;
                    details.push(format!("test{i}: PASS"));
                }
                Ok(false) => details.push(format!("test{i}: FAIL")),
                Err(e) => details.push(format!("test{i}: ERROR {e}")),
            }
        }
        let total = tests.len();
        let score = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };
        Ok(TestResult {
            passed,
            total,
            score,
            details,
        })
    }

    async fn llm_generate_tests(&self, code: &str, language: &str) -> Result<Vec<TestCase>> {
        let prompt = format!(
            "Generate {} test cases for the following {} code. \
Return ONLY a JSON array of objects with keys 'input' and 'expected_output'. \
Do not include explanations.\n\n```{}\n{}\n```",
            self.config.num_tests, language, language, code
        );
        let resp = self.client.mutate_with_prompt(&prompt).await?;
        let json_str = extract_json_array(&resp)?;
        let tests: Vec<TestCase> = serde_json::from_str(&json_str)
            .with_context(|| "LLM returned invalid JSON for tests")?;
        Ok(tests)
    }

    fn random_tests(&self, _code: &str, language: &str) -> Vec<TestCase> {
        let mut rng = rand::thread_rng();
        let mut tests = Vec::new();
        for _ in 0..self.config.num_tests {
            let input: String = match language {
                "python" | "py" | "rust" | "rs" | "go" | "javascript" | "js" => {
                    let n: i32 = rng.gen_range(0..1000);
                    format!("{}", n)
                }
                _ => "42".into(),
            };
            tests.push(TestCase {
                input,
                expected_output: None,
            });
        }
        tests
    }

    async fn run_single_test(
        &self,
        code: &str,
        language: &str,
        work_dir: &Path,
        sandbox_config: &SandboxConfig,
        test: &TestCase,
    ) -> Result<bool> {
        let wrapped = wrap_test_code(code, language, &test.input);
        let out = run_in_sandbox(&wrapped, language, work_dir, sandbox_config).await?;
        if !out.status.success() {
            return Ok(false);
        }
        let stdout_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(ref expected) = test.expected_output {
            Ok(stdout_str == expected.trim())
        } else {
            Ok(true) // no crash = pass
        }
    }
}

fn extract_json_array(text: &str) -> Result<String> {
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            return Ok(text[start..=end].to_string());
        }
    }
    anyhow::bail!("No JSON array found in response")
}

fn wrap_test_code(code: &str, language: &str, input: &str) -> String {
    match language {
        "python" | "py" => format!("{}\nprint(solve({}))\n", code, input),
        "javascript" | "js" => format!("{}\nconsole.log(solve({}))\n", code, input),
        "rust" | "rs" => format!(
            "{}\nfn main() {{ println!(\"{{}}\", solve({})); }}\n",
            code, input
        ),
        "go" => format!(
            "{}\nfunc main() {{ fmt.Println(solve({})) }}\n",
            code, input
        ),
        _ => format!("{}\n{}", code, input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_array() {
        let txt = r#"Some text [{"input":"1"}] more"#;
        assert_eq!(extract_json_array(txt).unwrap(), r#"[{"input":"1"}]"#);
    }

    #[test]
    fn test_random_tests() {
        let config = TestGeneratorConfig::default();
        // LlmClient requires config, can't easily construct in test
        // Just test the static helpers
        assert_eq!(config.num_tests, 5);
    }

    #[test]
    fn test_wrap_test_code() {
        let w = wrap_test_code("def solve(x): return x*2", "python", "3");
        assert!(w.contains("print(solve(3))"));
    }
}
