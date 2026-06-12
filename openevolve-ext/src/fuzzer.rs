//! Lightweight fuzzer: generates random inputs for evolved programs
//! and tests them against the evaluator for crash/edge case detection.

use rand::Rng;

pub struct Fuzzer;

impl Default for Fuzzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Fuzzer {
    pub fn new() -> Self {
        Self
    }

    /// Generate N random byte strings of varying length.
    pub fn fuzz_inputs(&mut self, count: usize) -> Vec<String> {
        let mut rng = rand::thread_rng();
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let len: usize = rng.gen_range(0..=256);
            let bytes: Vec<u8> = (0..len).map(|_| rng.gen()).collect();
            out.push(String::from_utf8_lossy(&bytes).to_string());
        }
        out
    }

    /// Run the evaluator against each fuzz input and report crashes.
    pub async fn run_fuzz(
        &mut self,
        eval_fn: impl Fn(&str) -> Result<f64, String>,
        inputs: &[String],
    ) -> Vec<(String, String)> {
        let mut crashes = Vec::new();
        for input in inputs {
            match eval_fn(input) {
                Ok(_score) => {}
                Err(e) => crashes.push((input.chars().take(40).collect(), e)),
            }
        }
        crashes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzz_inputs() {
        let mut f = Fuzzer::new();
        let inputs = f.fuzz_inputs(10);
        assert_eq!(inputs.len(), 10);
        // All are valid UTF-8
        for inp in &inputs {
            let _ = String::from_utf8(inp.as_bytes().to_vec()).expect("valid utf-8");
        }
    }
}
