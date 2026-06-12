//! Initial program to evolve. The evaluator looks for `solve(data: Vec<i64>) -> Vec<i64>`.
//!
//! The evolver MUST keep the function signature unchanged, and is expected to
//! only modify the body between EVOLVE-BLOCK markers. The starting algorithm
//! is a naive bubble sort which the engine should replace with something better.

use std::io::Read;

// EVOLVE-BLOCK-START
pub fn solve(mut data: Vec<i64>) -> Vec<i64> {
    let n = data.len();
    for i in 0..n {
        for j in 0..n.saturating_sub(i + 1) {
            if data[j] > data[j + 1] {
                data.swap(j, j + 1);
            }
        }
    }
    data
}
// EVOLVE-BLOCK-END

fn main() {
    // Reads one JSON array of integers from stdin, returns sorted JSON array on stdout.
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).expect("read stdin");
    let input: Vec<i64> = serde_json::from_str(buf.trim()).unwrap_or_default();
    let out = solve(input);
    println!("{}", serde_json::to_string(&out).unwrap());
}
