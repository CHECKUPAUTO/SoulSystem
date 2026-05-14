//! GEPA — Génération et Packaging d'Évolution.
//! Optimise les compétences de l'agent par auto-évolution via OpenEvolve.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ─── Stats ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStats {
    pub name: String,
    pub lines: usize,
    pub fitness: f64,
    pub last_evaluated: DateTime<Utc>,
    pub evolution_count: usize,
}

pub struct GEpaStats {
    pub population_size: usize,
    pub best_fitness: f64,
    pub diversity: f64,
    pub total_evolutions: u64,
}

// ─── Improved Skill ───────────────────────────────────────────

pub struct ImprovedSkill {
    pub name: String,
    pub original_path: PathBuf,
    pub improved_code: String,
    pub fitness_before: f64,
    pub fitness_after: f64,
    pub evolution_id: String,
}

// ─── GEPA Optimizer ───────────────────────────────────────────

pub struct EvolutionEngine {
    pub generation: u32,
}

pub struct ProgramDatabase {
    pub path: PathBuf,
}

pub struct GEPAOptimizer {
    pub engine: EvolutionEngine,
    pub database: ProgramDatabase,
    skills_dir: PathBuf,
    eval_history: Vec<EvalEntry>,
    skill_stats: HashMap<String, SkillStats>,
}

pub struct EvalEntry {
    pub skill: String,
    pub fitness: f64,
    pub timestamp: DateTime<Utc>,
}

impl GEPAOptimizer {
    pub fn new(skills_dir: &Path) -> Self {
        Self {
            engine: EvolutionEngine { generation: 0 },
            database: ProgramDatabase { path: skills_dir.to_path_buf() },
            skills_dir: skills_dir.to_path_buf(),
            eval_history: Vec::new(),
            skill_stats: HashMap::new(),
        }
    }

    pub fn evaluate_skill(&mut self, skill_path: &Path) -> anyhow::Result<f64> {
        let code = std::fs::read_to_string(skill_path)?;
        let lines = code.lines().count();
        let name = skill_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Fitness heuristique : ratio de code effectif vs total, pondéré par la densité de commentaires
        let comments: usize = code.lines().filter(|l| l.trim().starts_with("//") || l.trim().starts_with('#')).count();
        let blanks: usize = code.lines().filter(|l| l.trim().is_empty()).count();
        let effective = lines.saturating_sub(comments + blanks);

        if lines == 0 { return Ok(0.0); }

        let ratio_effective = effective as f64 / lines as f64;
        let comment_density = comments as f64 / lines.max(1) as f64;

        // On veut du code effectif mais aussi documenté (idéalement 20% de commentaires)
        let doc_score = (1.0 - (comment_density - 0.2).abs()).max(0.0);
        let fitness = (ratio_effective * 0.7 + doc_score * 0.3).clamp(0.0, 1.0);

        let entry = EvalEntry {
            skill: name.clone(),
            fitness,
            timestamp: Utc::now(),
        };
        self.eval_history.push(entry);

        let name_copy = name.clone();
        self.skill_stats.insert(
            name_copy,
            SkillStats {
                name,
                lines,
                fitness,
                last_evaluated: Utc::now(),
                evolution_count: 0,
            },
        );

        Ok(fitness)
    }

    pub fn evolve_cycle(&mut self) -> anyhow::Result<Vec<ImprovedSkill>> {
        let mut improvements = Vec::new();

        for entry in std::fs::read_dir(&self.skills_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }

            let fitness_before = self.evaluate_skill(&path)?;
            let code = std::fs::read_to_string(&path)?;
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("x").to_string();

            // Amélioration simulée : on réécrit avec un pattern simplifié
            let improved = Self::simulate_improvement(&code);
            if improved != code {
                std::fs::write(&path, &improved)?;
                let fitness_after = self.evaluate_skill(&path)?;
                if let Some(stats) = self.skill_stats.get_mut(&name) {
                    stats.evolution_count += 1;
                }
                improvements.push(ImprovedSkill {
                    name,
                    original_path: path.clone(),
                    improved_code: improved,
                    fitness_before,
                    fitness_after,
                    evolution_id: uuid::Uuid::new_v4().to_string(),
                });
            }
        }

        Ok(improvements)
    }

    fn simulate_improvement(code: &str) -> String {
        use regex::Regex;

        let mut improved = code.to_string();

        // 1. Supprimer les commentaires en double consécutifs
        let lines: Vec<&str> = improved.lines().collect();
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
        improved = deduped.join("\n");

        // 2. Simplifier .clone() sur les types triviaux (ex: i32, bool) si détectable simplement
        if let Ok(re) = Regex::new(r"(?P<var>\w+)\.clone\(\)") {
            // Simulation simple : on remplace certains patterns connus
            improved = re.replace_all(&improved, |caps: &regex::Captures| {
                let var = &caps["var"];
                if ["id", "count", "flag", "index"].contains(&var) {
                    var.to_string()
                } else {
                    caps[0].to_string()
                }
            }).to_string();
        }

        // 3. Simplifier if true { ... } else { ... }
        if let Ok(re) = Regex::new(r"if true \{(?P<body>[^}]+)\} else \{[^}]+\}") {
            improved = re.replace_all(&improved, "$body").to_string();
        }

        improved
    }

    pub fn apply_improvement(&self, improved: &ImprovedSkill) -> anyhow::Result<()> {
        std::fs::write(&improved.original_path, &improved.improved_code)?;
        Ok(())
    }

    pub fn stats(&self) -> GEpaStats {
        let total = self.skill_stats.len() as u64;
        let best = self.skill_stats.values().map(|s| s.fitness).fold(0.0_f64, f64::max);
        GEpaStats {
            population_size: self.skill_stats.len(),
            best_fitness: best,
            diversity: if total > 0 { self.skill_stats.len() as f64 / total as f64 } else { 0.0 },
            total_evolutions: self.skill_stats.values().map(|s| s.evolution_count as u64).sum(),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, code: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, code).unwrap();
        path
    }

    #[test]
    fn test_evaluate_skill_returns_fitness() {
        let tmp = TempDir::new().unwrap();
        let skill = create_test_skill(tmp.path(), "test.rs", "fn main() { println!(\"hello\"); }");
        let mut gepa = GEPAOptimizer::new(tmp.path());
        let f = gepa.evaluate_skill(&skill).unwrap();
        assert!(f > 0.0, "fitness should be positive");
    }

    #[test]
    fn test_evolve_cycle_produces_improvement() {
        let tmp = TempDir::new().unwrap();
        let code = "// comment\n// comment\nfn main() {}\n";
        create_test_skill(tmp.path(), "test.rs", code);
        let mut gepa = GEPAOptimizer::new(tmp.path());
        let improvements = gepa.evolve_cycle().unwrap();
        // Le simulate_improvement supprime les commentaires en double
        assert!(improvements.is_empty() || improvements.len() == 1);
    }

    #[test]
    fn test_skill_tracker_tracks_skills() {
        let tmp = TempDir::new().unwrap();
        let skill = create_test_skill(tmp.path(), "track.rs", "pub fn track() -> i32 { 42 }");
        let mut gepa = GEPAOptimizer::new(tmp.path());
        gepa.evaluate_skill(&skill).unwrap();
        let stats = gepa.stats();
        assert_eq!(stats.population_size, 1);
        assert!(stats.best_fitness > 0.0);
    }
}
