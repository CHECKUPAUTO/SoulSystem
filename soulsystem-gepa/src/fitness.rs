use anyhow::Result;
use std::path::Path;

/// Evaluate the fitness of a skill file.
/// Returns a score between 0.0 and 1.0 based on code quality heuristics.
pub fn evaluate_skill(skill_path: &Path) -> Result<f64> {
    let code = std::fs::read_to_string(skill_path)?;
    let lines = code.lines().count();

    // Heuristic: ratio of effective code vs total, weighted by comment density
    let comments: usize = code
        .lines()
        .filter(|l| l.trim().starts_with("//") || l.trim().starts_with('#'))
        .count();
    let blanks: usize = code.lines().filter(|l| l.trim().is_empty()).count();
    let effective = lines.saturating_sub(comments + blanks);

    if lines == 0 {
        return Ok(0.0);
    }

    let ratio_effective = effective as f64 / lines as f64;
    let comment_density = comments as f64 / lines.max(1) as f64;

    // Ideal comment density is ~20%
    let doc_score = (1.0 - (comment_density - 0.2).abs()).max(0.0);
    let fitness = (ratio_effective * 0.7 + doc_score * 0.3).clamp(0.0, 1.0);

    Ok(fitness)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_evaluate_empty_file() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("empty.rs");
        std::fs::write(&p, "").unwrap();
        let f = evaluate_skill(&p).unwrap();
        assert_eq!(f, 0.0);
    }

    #[test]
    fn test_evaluate_normal_file() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("hello.rs");
        std::fs::write(&p, "fn hello() -> i32 { 42 }").unwrap();
        let f = evaluate_skill(&p).unwrap();
        assert!(f > 0.0);
    }
}
