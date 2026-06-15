//! Concrete evaluators.
//!
//! [`CargoEvaluator`] is the real validation gate: it builds the candidate
//! workspace and runs its test suite, deriving fitness from "does it compile"
//! and "how many tests pass". [`ClosureEvaluator`] is a lightweight stand-in for
//! unit tests and custom domain rewards.

use crate::error::{Result, RsiError};
use crate::traits::Evaluator;
use crate::variant::Fitness;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds and tests a candidate workspace with `cargo`.
pub struct CargoEvaluator {
    /// Manifest directory inside the snapshot to build (relative to its root).
    pub package_subdir: PathBuf,
    /// Extra args passed to `cargo test` (e.g. `-p some_crate`).
    pub test_args: Vec<String>,
    /// Optional scalar reward on top of the compile/test gate (e.g. a benchmark
    /// number scraped from test output). Defaults to test pass-rate.
    pub score_from_passrate: bool,
}

impl Default for CargoEvaluator {
    fn default() -> Self {
        Self {
            package_subdir: PathBuf::new(),
            test_args: Vec::new(),
            score_from_passrate: true,
        }
    }
}

impl Evaluator for CargoEvaluator {
    fn evaluate(&self, workspace: &Path) -> Result<Fitness> {
        let dir = workspace.join(&self.package_subdir);

        // 1. Compile gate.
        let build = Command::new("cargo")
            .arg("build")
            .arg("--quiet")
            .current_dir(&dir)
            .output()
            .map_err(|e| RsiError::Evaluation(format!("cargo build: {e}")))?;
        if !build.status.success() {
            let err = String::from_utf8_lossy(&build.stderr);
            return Ok(Fitness::broken(format!(
                "build failed:\n{}",
                tail(&err, 1500)
            )));
        }

        // 2. Test gate.
        let mut cmd = Command::new("cargo");
        cmd.arg("test").arg("--quiet").current_dir(&dir);
        for a in &self.test_args {
            cmd.arg(a);
        }
        let test = cmd
            .output()
            .map_err(|e| RsiError::Evaluation(format!("cargo test: {e}")))?;
        let out = format!(
            "{}{}",
            String::from_utf8_lossy(&test.stdout),
            String::from_utf8_lossy(&test.stderr)
        );
        let (passed, failed) = parse_test_counts(&out);

        let score = if self.score_from_passrate {
            let total = passed + failed;
            if total == 0 {
                0.0
            } else {
                passed as f64 / total as f64
            }
        } else {
            0.0
        };

        Ok(Fitness {
            compiles: true,
            tests_passed: passed,
            tests_failed: failed,
            score,
            notes: if test.status.success() {
                "all tests passed".to_string()
            } else {
                format!("tests failed:\n{}", tail(&out, 1500))
            },
        })
    }
}

/// Sum the `test result: ok. N passed; M failed` lines cargo emits per binary.
///
/// The count keyword (`passed`/`failed`) is preceded by its number and possibly
/// by `ok.`/`FAILED.`, so we scan adjacent token pairs rather than suffixes.
fn parse_test_counts(output: &str) -> (u32, u32) {
    let mut passed = 0u32;
    let mut failed = 0u32;
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with("test result:") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        for pair in tokens.windows(2) {
            let kind = pair[1].trim_end_matches(';');
            match kind {
                "passed" => passed += pair[0].parse().unwrap_or(0),
                "failed" => failed += pair[0].parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    (passed, failed)
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let start = s.len() - max;
        // Avoid slicing mid-char.
        let start = (start..s.len())
            .find(|i| s.is_char_boundary(*i))
            .unwrap_or(s.len());
        format!("…{}", &s[start..])
    }
}

/// An evaluator backed by an arbitrary closure — handy for tests and for domain
/// rewards that do not need a full build (e.g. scoring an artifact directly).
pub struct ClosureEvaluator<F>
where
    F: Fn(&Path) -> Fitness,
{
    f: F,
}

impl<F> ClosureEvaluator<F>
where
    F: Fn(&Path) -> Fitness,
{
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F> Evaluator for ClosureEvaluator<F>
where
    F: Fn(&Path) -> Fitness,
{
    fn evaluate(&self, workspace: &Path) -> Result<Fitness> {
        Ok((self.f)(workspace))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_binary_test_output() {
        let out = "\
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 2 tests
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
";
        assert_eq!(parse_test_counts(out), (4, 1));
    }

    #[test]
    fn no_test_lines_is_zero() {
        assert_eq!(parse_test_counts("nothing here"), (0, 0));
    }

    #[test]
    fn closure_evaluator_runs() {
        let e = ClosureEvaluator::new(|_p: &Path| Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score: 9.0,
            notes: String::new(),
        });
        let tmp = tempfile::tempdir().unwrap();
        let f = e.evaluate(tmp.path()).unwrap();
        assert!(f.all_green());
        assert_eq!(f.score, 9.0);
    }
}
