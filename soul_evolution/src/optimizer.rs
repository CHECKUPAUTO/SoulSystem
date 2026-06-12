/// Module d'optimisation récursive de code Rust.
///
/// Implémente les trois prompts (Système, Cas A/B/C) fournis comme trames
/// de rétroaction pour la boucle d'optimisation de bas niveau.
///
/// Architecture:
///   Prompt Système → [LLM] → Code Rust → compile → test → bench
///       ↑                                              │
///       └────── Cas C (succès) ← Cas B (logique) ← Cas A (compile)
use std::fmt::Write;
use std::path::Path;
use std::time::Instant;

// ──────────────────────────────────────────
// 1. PROMPT SYSTÈME — Le Cerveau de la Boucle
// ──────────────────────────────────────────

pub const SYSTEM_PROMPT: &str = r#"Act as an autonomous Recursive Self-Improvement Agent specialized in low-level Rust systems engineering and LLVM micro-optimizations.

[MISSION]
Your sole objective is to optimize the provided Rust function to achieve the lowest possible execution latency (measured in nanoseconds).

[STRICT OUTPUT FORMAT]
- You must output EXACTLY and ONLY valid Rust code.
- Wrap your code strictly inside a single ```rust ... ``` markdown block.
- Absolutely NO conversational prose, NO explanations, NO comments outside the code block, and NO trailing text.
- Do not modify the function signature specified by the user.

[OPTIMIZATION GUIDELINES]
To beat the previous execution time, leverage hardware-level and compiler-level optimization vectors:
1. Instruction-Level Parallelism (ILP) & Loop Unrolling (manual or compiler hints via #[inline]).
2. SIMD Vectorization: Prefer native iterators that trigger auto-vectorization, or use safe intrinsics if applicable.
3. Cache Locality: Minimize memory footprints, prevent unnecessary allocations, and ensure sequential memory access patterns.
4. Branch Prediction: Eliminate branches in critical loops using bitwise operations, lookup tables, or conditionally executed math.
5. Compile-time Evaluation: Push computation to compile-time using 'const' contexts if the bounds are statically known."#;

// ──────────────────────────────────────────
// 2. PROMPTS DYNAMIQUES — Trames de Rétroaction
// ──────────────────────────────────────────

/// Cas A : Échec de compilation (rustc)
pub const CASE_A_COMPILE_FAILURE: &str = r#"[CRITICAL FAILURE: COMPILATION ERROR]
The previous mutation failed to compile. You introduced a regression or broke Rust's safety guarantees.

Detailed rustc output:
{error_log}

Task: Analyze the compiler error, fix the lifetime, ownership, type, or syntax issue immediately, and re-submit a corrected version. Maintain the optimization goal."#;

/// Cas B : Échec de validation logique (test unitaire)
pub const CASE_B_LOGIC_FAILURE: &str = r#"[CRITICAL FAILURE: LOGIC / VALIDATION ERROR]
The previous mutation compiled successfully, but failed verification during execution. The algorithm is incorrect or panicked.

Runtime feedback / Test log:
{error_log}

Task: You compromised the correctness of the algorithm in pursuit of speed. Fix the logical flaw to ensure valid and correct output, while keeping execution efficiency high."#;

/// Cas C : Succès & Optimisation (Feed-Forward)
pub const CASE_C_SUCCESS: &str = r#"[SUCCESS: PERFORMANCE METRICS]
The previous mutation passed validation and compiled successfully.
Current execution time: {execution_time_ns} ns.
Current record to beat: {best_time} ns.

Task: Analyze your previous implementation. Your goal is to compress the execution time further. Target a lower nanosecond threshold. Explore deeper hardware alignment, explicit compiler hints, or alternative mathematical formulations of the problem to unlock superior throughput."#;

// ──────────────────────────────────────────
// 3. STRUCTURES DE DONNÉES
// ──────────────────────────────────────────

/// Résultat d'une compilation
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub success: bool,
    pub error_log: String,
    pub duration_ms: u64,
}

/// Résultat d'un test
#[derive(Debug, Clone)]
pub struct TestResult {
    pub success: bool,
    pub error_log: String,
    pub duration_ms: u64,
}

/// Résultat d'un benchmark
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub execution_time_ns: f64,
    pub duration_ms: u64,
    pub iterations: u64,
}

/// Une tentative d'optimisation
#[derive(Debug, Clone)]
pub struct OptimizationAttempt {
    pub iteration: u64,
    pub code: String,
    pub compile: Option<CompileResult>,
    pub test: Option<TestResult>,
    pub bench: Option<BenchResult>,
    pub best_time_ns: f64,
    pub formatted_prompt: String,
}

/// L'état complet de la boucle d'optimisation
#[derive(Debug, Clone)]
pub struct OptimizationState {
    pub function_signature: String,
    pub original_code: String,
    pub test_code: String,
    pub crate_name: String,
    pub history: Vec<OptimizationAttempt>,
    pub best_time_ns: f64,
    pub iteration: u64,
    pub temp_dir: Option<String>,
}

// ──────────────────────────────────────────
// 4. FORMATAGE DES PROMPTS
// ──────────────────────────────────────────

impl OptimizationState {
    pub fn new(
        function_signature: &str,
        original_code: &str,
        test_code: &str,
        crate_name: &str,
    ) -> Self {
        Self {
            function_signature: function_signature.to_string(),
            original_code: original_code.to_string(),
            test_code: test_code.to_string(),
            crate_name: crate_name.to_string(),
            history: Vec::new(),
            best_time_ns: f64::INFINITY,
            iteration: 0,
            temp_dir: None,
        }
    }

    /// Construit le prompt système complet avec la fonction cible
    pub fn build_system_prompt(&self) -> String {
        format!(
            "{}\n\n[TARGET FUNCTION]\n```rust\n{}\n```\n\n[TEST]\n```rust\n{}\n```",
            SYSTEM_PROMPT, self.original_code, self.test_code
        )
    }

    /// Formate le prompt Cas A (échec compilation)
    pub fn format_case_a(error_log: &str) -> String {
        CASE_A_COMPILE_FAILURE.replace("{error_log}", error_log)
    }

    /// Formate le prompt Cas B (échec logique/test)
    pub fn format_case_b(error_log: &str) -> String {
        CASE_B_LOGIC_FAILURE.replace("{error_log}", error_log)
    }

    /// Formate le prompt Cas C (succès → feed-forward)
    pub fn format_case_c(execution_time_ns: f64, best_time: f64) -> String {
        CASE_C_SUCCESS
            .replace("{execution_time_ns}", &format!("{:.1}", execution_time_ns))
            .replace("{best_time}", &format!("{:.1}", best_time))
    }

    /// Ajoute une tentative à l'historique
    pub fn add_attempt(&mut self, attempt: OptimizationAttempt) {
        if let Some(ref bench) = attempt.bench {
            if bench.execution_time_ns < self.best_time_ns {
                self.best_time_ns = bench.execution_time_ns;
            }
        }
        self.iteration += 1;
        self.history.push(attempt);
    }
}

// ──────────────────────────────────────────
// 5. INFRASTRUCTURE DE COMPILATION/TEST/BENCH
// ──────────────────────────────────────────

pub struct Optimizer;

impl Optimizer {
    /// Compile du code Rust dans un projet cargo temporaire
    pub fn compile(
        code: &str,
        crate_name: &str,
        dependencies: Option<&[&str]>,
        temp_dir: &Path,
    ) -> CompileResult {
        let start = Instant::now();

        // Créer la structure du projet cargo
        let src_dir = temp_dir.join("src");
        if let Err(e) = std::fs::create_dir_all(&src_dir) {
            return CompileResult {
                success: false,
                error_log: format!("Cannot create temp dir: {}", e),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }

        // Écrire le Cargo.toml
        let mut cargo_toml = format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
"#,
            crate_name
        );

        if let Some(deps) = dependencies {
            if !deps.is_empty() {
                cargo_toml.push_str("\n[dependencies]\n");
                for dep in deps {
                    let _ = writeln!(cargo_toml, "{}", dep);
                }
            }
        }

        let _ = std::fs::write(temp_dir.join("Cargo.toml"), &cargo_toml);

        // Écrire le code source
        let _ = std::fs::write(src_dir.join("lib.rs"), code);

        // Lancer cargo build --release
        let output = std::process::Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(temp_dir)
            .output();

        let elapsed = start.elapsed().as_millis() as u64;

        match output {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() {
                    CompileResult {
                        success: true,
                        error_log: String::new(),
                        duration_ms: elapsed,
                    }
                } else {
                    CompileResult {
                        success: false,
                        error_log: stderr.to_string(),
                        duration_ms: elapsed,
                    }
                }
            }
            Err(e) => CompileResult {
                success: false,
                error_log: format!("Cargo execution failed: {}", e),
                duration_ms: elapsed,
            },
        }
    }

    /// Exécute les tests unitaires du crate
    pub fn run_tests(temp_dir: &Path) -> TestResult {
        let start = Instant::now();

        let output = std::process::Command::new("cargo")
            .args(["test", "--release"])
            .current_dir(temp_dir)
            .output();

        let elapsed = start.elapsed().as_millis() as u64;

        match output {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                let combined = format!("{}{}", stdout, stderr);

                if out.status.success() {
                    TestResult {
                        success: true,
                        error_log: String::new(),
                        duration_ms: elapsed,
                    }
                } else {
                    TestResult {
                        success: false,
                        error_log: combined,
                        duration_ms: elapsed,
                    }
                }
            }
            Err(e) => TestResult {
                success: false,
                error_log: format!("Test execution failed: {}", e),
                duration_ms: elapsed,
            },
        }
    }

    /// Benchmarks une fonction en mesurant le temps d'exécution
    ///
    /// Compile un binaire de benchmark qui appelle la fonction N fois
    /// et mesure le temps moyen par appel en nanosecondes.
    pub fn benchmark(
        function_name: &str,
        benchmark_code: &str,
        crate_name: &str,
        temp_dir: &Path,
        iterations: u64,
    ) -> BenchResult {
        let start = Instant::now();

        // Créer un binaire de benchmark
        let bench_code = format!(
            r#"use std::time::Instant;

{benchmark_code}

fn main() {{
    let iterations = {iterations};
    let start = Instant::now();

    for _ in 0..iterations {{
        let _ = {fn_name}();
    }}

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    println!("{{:.1}}", avg_ns);
}}"#,
            benchmark_code = benchmark_code,
            iterations = iterations,
            fn_name = function_name,
        );

        let src_dir = temp_dir.join("src");
        let _ = std::fs::write(src_dir.join("main.rs"), &bench_code);

        // Compiler en release
        let build = std::process::Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(temp_dir)
            .output();

        if let Err(_e) = build {
            return BenchResult {
                execution_time_ns: -1.0,
                duration_ms: start.elapsed().as_millis() as u64,
                iterations,
            };
        }

        // Exécuter le benchmark
        let binary = temp_dir.join("target").join("release").join(crate_name);
        let output = std::process::Command::new(&binary)
            .output();

        let elapsed = start.elapsed().as_millis() as u64;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let time_ns: f64 = stdout.trim().parse().unwrap_or(-1.0);
                BenchResult {
                    execution_time_ns: time_ns,
                    duration_ms: elapsed,
                    iterations,
                }
            }
            Err(_e) => BenchResult {
                execution_time_ns: -1.0,
                duration_ms: elapsed,
                iterations,
            },
        }
    }

    /// Construit le prompt de retour approprié selon le résultat
    pub fn build_feedback_prompt(
        compile: &CompileResult,
        test: Option<&TestResult>,
        bench: Option<&BenchResult>,
        best_time_ns: f64,
    ) -> String {
        // Cas A : Échec de compilation
        if !compile.success {
            return Self::build_iteration_prompt(&OptimizationState::format_case_a(&compile.error_log));
        }

        // Cas B : Échec de test
        if let Some(t) = test {
            if !t.success {
                return Self::build_iteration_prompt(&OptimizationState::format_case_b(&t.error_log));
            }
        }

        // Cas C : Succès → Feed-Forward
        if let Some(b) = bench {
            let time = b.execution_time_ns;
            Self::build_iteration_prompt(&OptimizationState::format_case_c(time, best_time_ns))
        } else {
            Self::build_iteration_prompt("All checks passed. Proceed with optimization.")
        }
    }

    fn build_iteration_prompt(feedback: &str) -> String {
        format!(
            "{}\n\n[FEEDBACK]\n{}",
            SYSTEM_PROMPT,
            feedback
        )
    }

    /// Nettoie le répertoire temporaire
    pub fn cleanup(temp_dir: &Path) {
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}

// ──────────────────────────────────────────
// 6. TESTS
// ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_contains_key_elements() {
        assert!(SYSTEM_PROMPT.contains("Recursive Self-Improvement"));
        assert!(SYSTEM_PROMPT.contains("ILP"));
        assert!(SYSTEM_PROMPT.contains("SIMD"));
        assert!(SYSTEM_PROMPT.contains("```rust"));
    }

    #[test]
    fn test_case_a_formatting() {
        let error = "error[E0382]: borrow of moved value";
        let prompt = OptimizationState::format_case_a(error);
        assert!(prompt.contains(error));
        assert!(prompt.contains("COMPILATION ERROR"));
    }

    #[test]
    fn test_case_b_formatting() {
        let error = "thread 'main' panicked at 'assertion failed'";
        let prompt = OptimizationState::format_case_b(error);
        assert!(prompt.contains(error));
        assert!(prompt.contains("LOGIC / VALIDATION ERROR"));
    }

    #[test]
    fn test_case_c_formatting() {
        let prompt = OptimizationState::format_case_c(42.5, 100.0);
        assert!(prompt.contains("42.5"));
        assert!(prompt.contains("100.0"));
        assert!(prompt.contains("SUCCESS"));
    }

    #[test]
    fn test_feedback_prompt_case_a() {
        let compile = CompileResult {
            success: false,
            error_log: "compile error".to_string(),
            duration_ms: 100,
        };
        let prompt = Optimizer::build_feedback_prompt(&compile, None, None, 100.0);
        assert!(prompt.contains("COMPILATION ERROR"));
    }

    #[test]
    fn test_feedback_prompt_case_b() {
        let compile = CompileResult {
            success: true,
            error_log: String::new(),
            duration_ms: 100,
        };
        let test = TestResult {
            success: false,
            error_log: "test failed".to_string(),
            duration_ms: 50,
        };
        let prompt = Optimizer::build_feedback_prompt(&compile, Some(&test), None, 100.0);
        assert!(prompt.contains("LOGIC / VALIDATION ERROR"));
    }

    #[test]
    fn test_feedback_prompt_case_c() {
        let compile = CompileResult {
            success: true,
            error_log: String::new(),
            duration_ms: 100,
        };
        let test = TestResult {
            success: true,
            error_log: String::new(),
            duration_ms: 50,
        };
        let bench = BenchResult {
            execution_time_ns: 42.0,
            duration_ms: 500,
            iterations: 10000,
        };
        let prompt = Optimizer::build_feedback_prompt(&compile, Some(&test), Some(&bench), 100.0);
        assert!(prompt.contains("SUCCESS"));
        assert!(prompt.contains("42.0"));
    }

    #[test]
    fn test_optimization_state_tracks_best_time() {
        let mut state = OptimizationState::new("fn foo()", "fn foo() {}", "#[test] fn t() {}", "test_crate");

        let mut a1 = OptimizationAttempt {
            iteration: 1,
            code: "fn foo() {}".to_string(),
            compile: None,
            test: None,
            bench: Some(BenchResult { execution_time_ns: 100.0, duration_ms: 0, iterations: 0 }),
            best_time_ns: f64::INFINITY,
            formatted_prompt: String::new(),
        };
        a1.best_time_ns = 100.0;
        state.add_attempt(a1);
        assert_eq!(state.best_time_ns, 100.0);

        let mut a2 = OptimizationAttempt {
            iteration: 2,
            code: "fn foo() {}".to_string(),
            compile: None,
            test: None,
            bench: Some(BenchResult { execution_time_ns: 50.0, duration_ms: 0, iterations: 0 }),
            best_time_ns: 100.0,
            formatted_prompt: String::new(),
        };
        a2.best_time_ns = 50.0;
        state.add_attempt(a2);
        assert_eq!(state.best_time_ns, 50.0);
    }
}
