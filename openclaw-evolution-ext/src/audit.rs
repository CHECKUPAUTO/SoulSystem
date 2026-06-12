//! # Système d'Audit — Vérification de Production Réelle
//!
//! Force le système à prouver qu'il produit des évolutions de code concrètes.
//! Pas de théâtre : si rien ne compile, l'audit échoue et le système est alerté.
//!
//! L'audit vérifie à chaque cycle :
//! 1. PRODUCTION : Des mutations de code ont-elles été générées ?
//! 2. COMPILATION : Le code produit compile-t-il ?
//! 3. EXÉCUTION : Le code compilé tourne-t-il ?
//! 4. PROGRÈS : La fitness monte-t-elle réellement ?
//! 5. DIVERSITÉ : Les mutations sont-elles variées (pas du copier-coller) ?
//! 6. INTÉGRITÉ : Le graphe de concepts, le MoE, les embeddings fonctionnent-ils ?
//!
//! Si l'audit échoue N fois de suite, il force un rollback + changement de stratégie.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

// ─────────────────────────────────────────────
// Verdicts d'audit
// ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuditVerdict {
    /// Tout va bien
    Pass,
    /// Des problèmes mineurs détectés
    Warning,
    /// Échec critique — le système ne produit rien de réel
    Fail,
}

impl std::fmt::Display for AuditVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditVerdict::Pass => write!(f, "✅ PASS"),
            AuditVerdict::Warning => write!(f, "⚠️  WARNING"),
            AuditVerdict::Fail => write!(f, "❌ FAIL"),
        }
    }
}

// ─────────────────────────────────────────────
// Checks individuels
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditCheck {
    pub name: String,
    pub verdict: AuditVerdict,
    pub detail: String,
    pub metric: f32,
    pub threshold: f32,
}

impl AuditCheck {
    pub fn pass(name: &str, detail: &str, metric: f32) -> Self {
        Self {
            name: name.to_string(),
            verdict: AuditVerdict::Pass,
            detail: detail.to_string(),
            metric,
            threshold: 0.0,
        }
    }

    pub fn warn(name: &str, detail: &str, metric: f32, threshold: f32) -> Self {
        Self {
            name: name.to_string(),
            verdict: AuditVerdict::Warning,
            detail: detail.to_string(),
            metric,
            threshold,
        }
    }

    pub fn fail(name: &str, detail: &str, metric: f32, threshold: f32) -> Self {
        Self {
            name: name.to_string(),
            verdict: AuditVerdict::Fail,
            detail: detail.to_string(),
            metric,
            threshold,
        }
    }
}

// ─────────────────────────────────────────────
// Rapport d'audit complet
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub cycle: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub overall_verdict: AuditVerdict,
    pub checks: Vec<AuditCheck>,
    /// Nombre de cycles consécutifs en échec
    pub consecutive_failures: usize,
    /// Action recommandée
    pub recommended_action: AuditAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    /// Continuer normalement
    Continue,
    /// Augmenter le taux de mutation
    IncreaseMutation,
    /// Basculer en mode DEEP
    SwitchToDeep,
    /// Forcer un rollback
    ForceRollback,
    /// Alerte critique — intervention humaine recommandée
    CriticalAlert,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditAction::Continue => write!(f, "CONTINUE"),
            AuditAction::IncreaseMutation => write!(f, "INCREASE_MUTATION"),
            AuditAction::SwitchToDeep => write!(f, "SWITCH_DEEP"),
            AuditAction::ForceRollback => write!(f, "FORCE_ROLLBACK"),
            AuditAction::CriticalAlert => write!(f, "CRITICAL_ALERT"),
        }
    }
}

// ─────────────────────────────────────────────
// Métriques d'entrée pour l'audit
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditInput {
    pub cycle: u64,
    /// Nombre de mutations de code générées ce cycle
    pub mutations_generated: usize,
    /// Nombre de mutations avec du code non-vide
    pub mutations_with_code: usize,
    /// Nombre d'agents dont le code compile
    pub agents_compiling: usize,
    /// Nombre d'agents dont le code tourne
    pub agents_running: usize,
    /// Taille de la population
    pub population_size: usize,
    /// Fitness moyenne actuelle
    pub avg_fitness: f32,
    /// Fitness moyenne du cycle précédent
    pub prev_avg_fitness: f32,
    /// Meilleure fitness
    pub best_fitness: f32,
    /// Nombre de codes uniques (diversité)
    pub unique_code_hashes: usize,
    /// Taille moyenne du code en lignes
    pub avg_code_lines: f32,
    /// Nœuds actifs dans le concept graph
    pub graph_active_nodes: usize,
    /// Arêtes fortes dans le concept graph
    pub graph_strong_edges: usize,
    /// Embeddings batch générés ce cycle
    pub embeddings_generated: u64,
    /// Organes MoE actifs
    pub moe_organs_active: usize,
    /// Mode d'inférence courant
    pub inference_mode: String,
}

// ─────────────────────────────────────────────
// Auditeur principal
// ─────────────────────────────────────────────

pub struct Auditor {
    /// Seuils de tolérance
    pub thresholds: AuditThresholds,
    /// Historique des rapports
    pub history: Vec<AuditReport>,
    /// Compteur d'échecs consécutifs
    pub consecutive_failures: usize,
    /// Répertoire des logs d'audit
    pub log_dir: PathBuf,
    /// Max rapports gardés en mémoire
    pub max_history: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditThresholds {
    /// % minimum de mutations avec du code
    pub min_code_ratio: f32,
    /// % minimum d'agents qui compilent
    pub min_compile_ratio: f32,
    /// % minimum d'agents qui tournent
    pub min_run_ratio: f32,
    /// Nombre minimum de codes uniques (diversité)
    pub min_unique_codes: usize,
    /// Nombre de cycles d'échec avant rollback
    pub max_consecutive_failures: usize,
    /// Fitness minimum attendue après N cycles
    pub min_fitness_after_cycles: Vec<(u64, f32)>,
    /// Nombre minimum de nœuds actifs dans le graph
    pub min_graph_nodes: usize,
}

impl Default for AuditThresholds {
    fn default() -> Self {
        Self {
            min_code_ratio: 0.3,       // 30% des mutations doivent avoir du code
            min_compile_ratio: 0.1,    // 10% des agents doivent compiler
            min_run_ratio: 0.05,       // 5% doivent tourner
            min_unique_codes: 3,       // Au moins 3 codes différents
            max_consecutive_failures: 5,
            min_fitness_after_cycles: vec![
                (10, 0.05),  // Après 10 cycles : fitness > 0.05
                (50, 0.15),  // Après 50 cycles : fitness > 0.15
                (100, 0.30), // Après 100 cycles : fitness > 0.30
            ],
            min_graph_nodes: 5,
        }
    }
}

impl Auditor {
    pub fn new(log_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&log_dir).ok();
        Self {
            thresholds: AuditThresholds::default(),
            history: Vec::new(),
            consecutive_failures: 0,
            log_dir,
            max_history: 200,
        }
    }

    /// Exécuter l'audit complet sur un cycle
    pub fn audit(&mut self, input: &AuditInput) -> AuditReport {
        let mut checks = Vec::new();

        // ── CHECK 1 : PRODUCTION — Des mutations de code ont-elles été générées ?
        let code_ratio = if input.mutations_generated > 0 {
            input.mutations_with_code as f32 / input.mutations_generated as f32
        } else {
            0.0
        };

        if input.mutations_generated == 0 {
            checks.push(AuditCheck::fail(
                "PRODUCTION",
                "Aucune mutation générée ce cycle",
                0.0,
                1.0,
            ));
        } else if code_ratio < self.thresholds.min_code_ratio {
            checks.push(AuditCheck::warn(
                "PRODUCTION",
                &format!(
                    "{}/{} mutations avec code ({:.0}% < seuil {:.0}%)",
                    input.mutations_with_code,
                    input.mutations_generated,
                    code_ratio * 100.0,
                    self.thresholds.min_code_ratio * 100.0
                ),
                code_ratio,
                self.thresholds.min_code_ratio,
            ));
        } else {
            checks.push(AuditCheck::pass(
                "PRODUCTION",
                &format!(
                    "{}/{} mutations avec code ({:.0}%)",
                    input.mutations_with_code, input.mutations_generated, code_ratio * 100.0
                ),
                code_ratio,
            ));
        }

        // ── CHECK 2 : COMPILATION — Le code compile-t-il ?
        let compile_ratio = if input.population_size > 0 {
            input.agents_compiling as f32 / input.population_size as f32
        } else {
            0.0
        };

        if compile_ratio < self.thresholds.min_compile_ratio && input.cycle > 5 {
            checks.push(AuditCheck::fail(
                "COMPILATION",
                &format!(
                    "{}/{} agents compilent ({:.0}% < seuil {:.0}%)",
                    input.agents_compiling,
                    input.population_size,
                    compile_ratio * 100.0,
                    self.thresholds.min_compile_ratio * 100.0
                ),
                compile_ratio,
                self.thresholds.min_compile_ratio,
            ));
        } else if compile_ratio > 0.0 {
            checks.push(AuditCheck::pass(
                "COMPILATION",
                &format!(
                    "{}/{} agents compilent ({:.0}%)",
                    input.agents_compiling, input.population_size, compile_ratio * 100.0
                ),
                compile_ratio,
            ));
        } else {
            checks.push(AuditCheck::warn(
                "COMPILATION",
                "Aucun agent ne compile encore (normal au démarrage)",
                0.0,
                self.thresholds.min_compile_ratio,
            ));
        }

        // ── CHECK 3 : EXÉCUTION — Le code tourne-t-il ?
        let run_ratio = if input.population_size > 0 {
            input.agents_running as f32 / input.population_size as f32
        } else {
            0.0
        };

        if run_ratio < self.thresholds.min_run_ratio && input.cycle > 10 {
            checks.push(AuditCheck::fail(
                "EXECUTION",
                &format!(
                    "{}/{} agents tournent ({:.0}% < seuil {:.0}%)",
                    input.agents_running,
                    input.population_size,
                    run_ratio * 100.0,
                    self.thresholds.min_run_ratio * 100.0
                ),
                run_ratio,
                self.thresholds.min_run_ratio,
            ));
        } else {
            let v = if run_ratio > 0.0 { AuditVerdict::Pass } else { AuditVerdict::Warning };
            checks.push(AuditCheck {
                name: "EXECUTION".into(),
                verdict: v,
                detail: format!("{}/{} agents tournent", input.agents_running, input.population_size),
                metric: run_ratio,
                threshold: self.thresholds.min_run_ratio,
            });
        }

        // ── CHECK 4 : PROGRÈS — La fitness monte-t-elle ?
        let fitness_delta = input.avg_fitness - input.prev_avg_fitness;

        // Vérifier les seuils par palier
        let expected_fitness = self
            .thresholds
            .min_fitness_after_cycles
            .iter()
            .filter(|(c, _)| input.cycle >= *c)
            .map(|(_, f)| *f)
            .last()
            .unwrap_or(0.0);

        if input.avg_fitness < expected_fitness && input.cycle > 10 {
            checks.push(AuditCheck::fail(
                "PROGRESS",
                &format!(
                    "Fitness {:.4} < attendu {:.4} au cycle {} (Δ={:+.4})",
                    input.avg_fitness, expected_fitness, input.cycle, fitness_delta
                ),
                input.avg_fitness,
                expected_fitness,
            ));
        } else if fitness_delta < -0.01 && input.cycle > 5 {
            checks.push(AuditCheck::warn(
                "PROGRESS",
                &format!(
                    "Fitness en baisse : {:.4} → {:.4} (Δ={:+.4})",
                    input.prev_avg_fitness, input.avg_fitness, fitness_delta
                ),
                fitness_delta,
                0.0,
            ));
        } else {
            checks.push(AuditCheck::pass(
                "PROGRESS",
                &format!(
                    "Fitness {:.4} (best={:.4}, Δ={:+.4})",
                    input.avg_fitness, input.best_fitness, fitness_delta
                ),
                input.avg_fitness,
            ));
        }

        // ── CHECK 5 : DIVERSITÉ — Pas de copier-coller
        if input.unique_code_hashes < self.thresholds.min_unique_codes && input.cycle > 10 {
            checks.push(AuditCheck::fail(
                "DIVERSITY",
                &format!(
                    "Seulement {} codes uniques (seuil={}). Le LLM produit du copier-coller.",
                    input.unique_code_hashes, self.thresholds.min_unique_codes
                ),
                input.unique_code_hashes as f32,
                self.thresholds.min_unique_codes as f32,
            ));
        } else {
            checks.push(AuditCheck::pass(
                "DIVERSITY",
                &format!("{} codes uniques, {:.1} lignes en moyenne", input.unique_code_hashes, input.avg_code_lines),
                input.unique_code_hashes as f32,
            ));
        }

        // ── CHECK 6 : INTÉGRITÉ SYSTÈME — Graph, MoE, Embeddings
        let mut integrity_issues = Vec::new();

        if input.graph_active_nodes < self.thresholds.min_graph_nodes && input.cycle > 5 {
            integrity_issues.push(format!("graph: {} nœuds actifs", input.graph_active_nodes));
        }
        if input.embeddings_generated == 0 && input.cycle > 2 {
            integrity_issues.push("embeddings: aucun généré".to_string());
        }

        if integrity_issues.is_empty() {
            checks.push(AuditCheck::pass(
                "INTEGRITY",
                &format!(
                    "Graph: {} nœuds/{} arêtes, MoE: {} organes, Embeds: {} générés",
                    input.graph_active_nodes,
                    input.graph_strong_edges,
                    input.moe_organs_active,
                    input.embeddings_generated
                ),
                1.0,
            ));
        } else {
            checks.push(AuditCheck::warn(
                "INTEGRITY",
                &format!("Problèmes: {}", integrity_issues.join(", ")),
                0.0,
                1.0,
            ));
        }

        // ── Verdict global ──────────────────────────
        let fails = checks.iter().filter(|c| c.verdict == AuditVerdict::Fail).count();
        let warns = checks.iter().filter(|c| c.verdict == AuditVerdict::Warning).count();

        let overall_verdict = if fails >= 2 {
            AuditVerdict::Fail
        } else if fails == 1 || warns >= 3 {
            AuditVerdict::Warning
        } else {
            AuditVerdict::Pass
        };

        // Mettre à jour le compteur d'échecs
        if overall_verdict == AuditVerdict::Fail {
            self.consecutive_failures += 1;
        } else {
            self.consecutive_failures = 0;
        }

        // Déterminer l'action recommandée
        let recommended_action = if self.consecutive_failures >= self.thresholds.max_consecutive_failures {
            AuditAction::ForceRollback
        } else if self.consecutive_failures >= 3 {
            AuditAction::SwitchToDeep
        } else if self.consecutive_failures >= 2 {
            AuditAction::IncreaseMutation
        } else if overall_verdict == AuditVerdict::Fail {
            AuditAction::IncreaseMutation
        } else {
            AuditAction::Continue
        };

        let report = AuditReport {
            cycle: input.cycle,
            timestamp: chrono::Utc::now(),
            overall_verdict: overall_verdict.clone(),
            checks,
            consecutive_failures: self.consecutive_failures,
            recommended_action,
        };

        // Log le rapport
        self.log_report(&report);

        // Sauvegarder sur disque
        self.save_report(&report);

        // Historique
        self.history.push(report.clone());
        if self.history.len() > self.max_history {
            self.history.drain(0..self.history.len() - self.max_history);
        }

        report
    }

    /// Logger le rapport d'audit
    fn log_report(&self, report: &AuditReport) {
        let header = format!(
            "AUDIT cycle {} — {} — échecs consécutifs: {} — action: {}",
            report.cycle,
            report.overall_verdict,
            report.consecutive_failures,
            report.recommended_action
        );

        match report.overall_verdict {
            AuditVerdict::Pass => info!("{}", header),
            AuditVerdict::Warning => warn!("{}", header),
            AuditVerdict::Fail => error!("{}", header),
        }

        for check in &report.checks {
            match check.verdict {
                AuditVerdict::Pass => info!("  {} {}: {}", check.verdict, check.name, check.detail),
                AuditVerdict::Warning => warn!("  {} {}: {}", check.verdict, check.name, check.detail),
                AuditVerdict::Fail => error!("  {} {}: {}", check.verdict, check.name, check.detail),
            }
        }
    }

    /// Sauvegarder le rapport sur disque (JSON)
    fn save_report(&self, report: &AuditReport) {
        let filename = format!(
            "audit_cycle_{:06}_{}.json",
            report.cycle,
            if report.overall_verdict == AuditVerdict::Fail { "FAIL" } else { "ok" }
        );
        let path = self.log_dir.join(filename);
        if let Ok(json) = serde_json::to_string_pretty(report) {
            std::fs::write(&path, json).ok();
        }
    }

    /// Tendance : les N derniers rapports montrent-ils une amélioration ?
    pub fn trend(&self, window: usize) -> AuditTrend {
        if self.history.len() < 2 {
            return AuditTrend::Insufficient;
        }

        let recent = &self.history[self.history.len().saturating_sub(window)..];
        let passes = recent.iter().filter(|r| r.overall_verdict == AuditVerdict::Pass).count();
        let fails = recent.iter().filter(|r| r.overall_verdict == AuditVerdict::Fail).count();

        if passes as f32 / recent.len() as f32 > 0.7 {
            AuditTrend::Improving
        } else if fails as f32 / recent.len() as f32 > 0.5 {
            AuditTrend::Degrading
        } else {
            AuditTrend::Stable
        }
    }

    /// Résumé compact pour les logs
    pub fn summary(&self) -> String {
        if let Some(last) = self.history.last() {
            format!(
                "Audit: {} ({} checks, {} échecs consécutifs, trend={:?})",
                last.overall_verdict,
                last.checks.len(),
                self.consecutive_failures,
                self.trend(10)
            )
        } else {
            "Audit: pas encore de rapport".to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditTrend {
    Improving,
    Stable,
    Degrading,
    Insufficient,
}
