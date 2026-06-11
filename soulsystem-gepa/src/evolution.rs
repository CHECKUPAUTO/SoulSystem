use crate::backup;
use crate::fitness;
use crate::GepaConfig;

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use uuid::Uuid;
use walkdir::WalkDir;

// ─── Structs ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImprovedSkill {
    pub name: String,
    pub original_path: PathBuf,
    pub improved_code: String,
    pub fitness_before: f64,
    pub fitness_after: f64,
    pub evolution_id: String,
}

#[derive(Debug, Clone)]
pub struct SkillStats {
    pub name: String,
    pub lines: usize,
    pub fitness: f64,
    pub evolution_count: usize,
}

#[derive(Debug)]
pub struct GEpaStats {
    pub population_size: usize,
    pub best_fitness: f64,
    pub total_evolutions: u64,
}

// ─── Evolution Engine ─────────────────────────────────────────

pub struct EvolutionEngine {
    config: GepaConfig,
    generation: u32,
    skill_stats: Vec<SkillStats>,
    improvements: Vec<ImprovedSkill>,
}

impl EvolutionEngine {
    pub fn new(config: GepaConfig) -> Self {
        Self {
            config,
            generation: 0,
            skill_stats: Vec::new(),
            improvements: Vec::new(),
        }
    }

    /// Main evolution loop over all .rs files under project_root.
    pub fn evolve(&mut self, project_root: &Path) -> Result<Vec<ImprovedSkill>> {
        let skills: Vec<PathBuf> = WalkDir::new(project_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("rs")
                    && !e.path().to_string_lossy().contains("target")
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        info!("Found {} Rust files to evolve", skills.len());

        for generation in 0..self.config.max_generations {
            self.generation = generation + 1;
            let mut gen_improvements = Vec::new();

            for skill_path in &skills {
                let fitness_before = fitness::evaluate_skill(skill_path).unwrap_or(0.0);

                // Skip skills below threshold
                if fitness_before < self.config.threshold {
                    debug!(
                        "Skipping {:?} — fitness {:.2} below threshold {:.2}",
                        skill_path, fitness_before, self.config.threshold
                    );
                    continue;
                }

                let code = std::fs::read_to_string(skill_path)?;

                // Backup before mutation
                if self.config.backup_enabled {
                    backup::backup_file(skill_path)?;
                }

                let transformer = ImprovementTransformer::new();
                let improved_code = transformer.transform(&code);

                if improved_code != code {
                    // Write improved code
                    std::fs::write(skill_path, &improved_code)?;

                    let fitness_after = fitness::evaluate_skill(skill_path).unwrap_or(0.0);
                    let name = skill_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let improved = ImprovedSkill {
                        name: name.clone(),
                        original_path: skill_path.clone(),
                        improved_code,
                        fitness_before,
                        fitness_after,
                        evolution_id: Uuid::new_v4().to_string(),
                    };

                    info!(
                        "Evolved {}: {:.3} → {:.3}",
                        name, fitness_before, fitness_after
                    );

                    gen_improvements.push(improved);

                    self.skill_stats.push(SkillStats {
                        name,
                        lines: skill_stats_from_path(skill_path),
                        fitness: fitness_after,
                        evolution_count: self.generation as usize,
                    });
                }
            }

            self.improvements.extend(gen_improvements);

            info!(
                "Generation {} complete — {} improvements",
                self.generation,
                self.improvements.len()
            );
        }

        let result = self.improvements.clone();
        Ok(result)
    }

    pub fn stats(&self) -> GEpaStats {
        let best = self
            .skill_stats
            .iter()
            .map(|s| s.fitness)
            .fold(0.0_f64, f64::max);
        GEpaStats {
            population_size: self.skill_stats.len(),
            best_fitness: best,
            total_evolutions: self.improvements.len() as u64,
        }
    }
}

fn skill_stats_from_path(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

// ─── Improvement Transformer ──────────────────────────────────

pub struct ImprovementTransformer;

impl Default for ImprovementTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl ImprovementTransformer {
    pub fn new() -> Self {
        Self
    }

    /// Apply a set of heuristic improvements to Rust source code.
    pub fn transform(&self, code: &str) -> String {
        let mut code = code.to_string();

        // 1. Remove consecutive duplicate comment lines
        code = remove_duplicate_comments(&code);

        // 2. Simplify redundant .clone() on Copy types
        code = simplify_clone(&code);

        // 3. Simplify if true { ... } else { ... }
        code = simplify_if_true(&code);

        code
    }
}

// ─── Heuristic Transforms ─────────────────────────────────────

fn remove_duplicate_comments(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let mut deduped = Vec::new();
    let mut prev = "";
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("//") && trimmed == prev {
            continue;
        }
        deduped.push(*line);
        prev = trimmed;
    }
    deduped.join("\n")
}

fn simplify_clone(code: &str) -> String {
    let re = regex::Regex::new(r"(?P<var>\w+)\.clone\(\)").unwrap();
    re.replace_all(code, |caps: &regex::Captures| {
        let var = &caps["var"];
        if ["id", "count", "flag", "index", "len", "n"].contains(&var) {
            var.to_string()
        } else {
            caps[0].to_string()
        }
    })
    .to_string()
}

fn simplify_if_true(code: &str) -> String {
    let re = regex::Regex::new(r"if true \{(?P<body>[^}]+)\} else \{[^}]+\}").unwrap();
    re.replace_all(code, "$body").to_string()
}

// ─── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_removes_duplicate_comments() {
        let input = "// hello\n// hello\nfn main() {}";
        let t = ImprovementTransformer::new();
        let out = t.transform(input);
        assert_eq!(out, "// hello\nfn main() {}");
    }

    #[test]
    fn test_engine_can_instantiate() {
        let config = GepaConfig::default();
        let engine = EvolutionEngine::new(config);
        assert_eq!(engine.generation, 0);
    }
}
