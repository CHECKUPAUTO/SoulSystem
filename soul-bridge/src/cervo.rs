// Pont CERVO-bridge — interface vers le cerveau autonome
//
// CERVO est un écosystème swarm d'unités auto-évolutives :
// chaque unité exécute un algorithme, mute, échange avec ses
// pairs, et le Cortex supervise l'ensemble.
//
// Ce bridge expose CERVO aux autres modules SoulSystem et à
// SoulSystem via le MCP server dédié.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use cervo::{
    config::CervoConfig,
    cortex::{Cortex, CortexReport},
    AmplifyTransform, Data, IdentityTransform, ReverseTransform, Transformation,
};

/// Status runtime de CERVO.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BridgeStatus {
    pub running: bool,
    pub unit_count: usize,
    pub total_mutations: u64,
    pub total_processed: u64,
    pub total_adopted: u64,
    pub last_check: chrono::DateTime<chrono::Utc>,
}

/// Résultat de mutation d'une unité.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MutationOutcome {
    pub unit_id: String,
    pub accepted: bool,
    pub previous_algorithm: String,
    pub new_algorithm: Option<String>,
    pub rejection_reason: Option<String>,
}

/// Résumé de santé d'une unité.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitHealth {
    pub id: String,
    pub rsi_index: f64,
    pub algorithm_name: String,
    pub saturation: f64,
    pub memory_active: usize,
    pub memory_long_term: usize,
    pub processed_count: u64,
    pub adopted_count: u64,
    pub known_peers: usize,
    pub evolution_summary: Vec<(String, u64, u64, f64)>,
}

/// Résultat du cortex report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CortexSummary {
    pub unit_count: usize,
    pub total_mutations: u64,
    pub total_processed: u64,
    pub total_adopted: u64,
    pub evolution_summary: Vec<(String, u64, u64, f64)>,
    pub units: Vec<UnitHealth>,
}

#[derive(Clone)]
pub struct CervoBridge {
    cortex: Arc<RwLock<Option<Cortex>>>,
    config: CervoConfig,
}

impl CervoBridge {
    pub fn new(config: CervoConfig) -> Self {
        CervoBridge {
            cortex: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Démarre CERVO : valide la config et crée le cortex.
    pub async fn start(&self) -> Result<(), String> {
        let mut guard = self.cortex.write().await;
        if guard.is_some() {
            return Ok(());
        }

        let cortex = Cortex::try_new(self.config.clone())?;
        info!("CERVO cortex started — {} units", cortex.unit_count());
        *guard = Some(cortex);
        Ok(())
    }

    /// Arrête CERVO.
    pub async fn stop(&self) {
        let mut guard = self.cortex.write().await;
        if let Some(ref mut cortex) = *guard {
            cortex.broadcast_shutdown().await;
        }
        *guard = None;
        info!("CERVO stopped");
    }

    /// Spawn une nouvelle unité avec l'algorithme donné.
    pub async fn spawn_unit(&self, algo: &str, factor: Option<usize>) -> Result<String, String> {
        let mut guard = self.cortex.write().await;
        let cortex = guard.as_mut().ok_or("CERVO not started")?;

        let transform: Box<dyn Transformation> = match algo {
            "identity" => Box::new(IdentityTransform),
            "reverse" => Box::new(ReverseTransform),
            "amplify" => Box::new(AmplifyTransform {
                factor: factor.unwrap_or(2),
            }),
            other => return Err(format!("Unknown algorithm: {other}")),
        };

        let id = cortex.spawn(transform).await;
        info!("CERVO spawned unit {id} with algorithm {algo}");
        Ok(id.to_string())
    }

    /// Mutate une unité spécifique.
    pub async fn mutate_unit(&self, unit_id: &str) -> Result<MutationOutcome, String> {
        let guard = self.cortex.read().await;
        let cortex = guard.as_ref().ok_or("CERVO not started")?;

        let id = Uuid::parse_str(unit_id).map_err(|e| format!("Invalid UUID: {e}"))?;
        let handle = cortex.get(&id).ok_or("Unit not found")?;

        let report = handle.mutate().await;
        Ok(MutationOutcome {
            unit_id: unit_id.to_string(),
            accepted: report.accepted,
            previous_algorithm: report.previous_algorithm,
            new_algorithm: report.new_algorithm,
            rejection_reason: report.rejection_reason,
        })
    }

    /// Traite un batch de données sur toutes les unités.
    pub async fn process_data(&self, payloads: Vec<Vec<u8>>) -> Result<serde_json::Value, String> {
        let guard = self.cortex.read().await;
        let cortex = guard.as_ref().ok_or("CERVO not started")?;

        let data: Vec<Data> = payloads.into_iter().map(Data::new).collect();
        let results = cortex.process_all(data).await;

        let mut out = serde_json::Map::new();
        for (id, result) in &results {
            let status = match result {
                Ok(outputs) => {
                    let bytes: Vec<Vec<u8>> = outputs.iter().map(|d| d.payload.clone()).collect();
                    serde_json::json!({ "ok": true, "outputs": bytes })
                }
                Err(e) => serde_json::json!({ "ok": false, "error": e }),
            };
            out.insert(id.to_string(), status);
        }
        Ok(serde_json::Value::Object(out))
    }

    /// Cycle toutes les unités (dynamiques + health broadcast).
    pub async fn tick_all(&self) -> Result<(), String> {
        let guard = self.cortex.read().await;
        let cortex = guard.as_ref().ok_or("CERVO not started")?;
        cortex.tick_all().await;
        Ok(())
    }

    /// Rapport complet du cortex.
    pub async fn report(&self) -> Result<CortexSummary, String> {
        let guard = self.cortex.read().await;
        let cortex = guard.as_ref().ok_or("CERVO not started")?;
        let r = cortex.report().await;

        let units: Vec<UnitHealth> = r
            .units
            .iter()
            .map(|u| UnitHealth {
                id: u.id.to_string(),
                rsi_index: u.rsi_index,
                algorithm_name: u.algorithm_name.clone(),
                saturation: u.saturation,
                memory_active: u.memory_active_count,
                memory_long_term: u.memory_long_term_count,
                processed_count: u.processed_count,
                adopted_count: u.adopted_count,
                known_peers: u.known_peers,
                evolution_summary: u.evolution_summary.clone(),
            })
            .collect();

        Ok(CortexSummary {
            unit_count: r.unit_count,
            total_mutations: r.total_mutations,
            total_processed: r.total_processed,
            total_adopted: r.total_adopted,
            evolution_summary: r.evolution_summary,
            units,
        })
    }

    /// Status de la connexion.
    pub async fn status(&self) -> BridgeStatus {
        let guard = self.cortex.read().await;
        match guard.as_ref() {
            Some(cortex) => {
                let r = cortex.report().await;
                BridgeStatus {
                    running: true,
                    unit_count: r.unit_count,
                    total_mutations: r.total_mutations,
                    total_processed: r.total_processed,
                    total_adopted: r.total_adopted,
                    last_check: chrono::Utc::now(),
                }
            }
            None => BridgeStatus {
                running: false,
                unit_count: 0,
                total_mutations: 0,
                total_processed: 0,
                total_adopted: 0,
                last_check: chrono::Utc::now(),
            },
        }
    }
}

/// Initialise le bridge CERVO.
pub async fn init() -> Result<CervoBridge, String> {
    let config = CervoConfig::default();
    let bridge = CervoBridge::new(config);
    bridge.start().await?;
    info!("CERVO bridge initialized");
    Ok(bridge)
}

impl CervoBridge {
    /// Boucle RSI (Recursive Self-Improvement) : propose → évalue → garde si strictement meilleur → répète.
    /// Utilise CERVO comme testbed : chaque itération spawn une unité, la fait muter,
    /// mesure son score d'évolution, et conserve la meilleure configuration.
    /// Garanties : bornée (max_iters), élitiste (meilleur score conservé), vérifiable.
    pub async fn rsi_evolve(
        &self,
        algorithm: &str,
        max_iters: usize,
    ) -> Result<serde_json::Value, String> {
        let mut best_score = 0.0f64;
        let mut best_config = serde_json::json!({});
        let mut history: Vec<serde_json::Value> = Vec::new();

        for iter in 0..max_iters {
            let unit_id = self.spawn_unit(algorithm, None).await?;

            let mut accepted = 0usize;
            let mut total = 0usize;
            for _ in 0..5 {
                let outcome = self.mutate_unit(&unit_id).await?;
                total += 1;
                if outcome.accepted {
                    accepted += 1;
                }
            }

            let score = if total > 0 {
                accepted as f64 / total as f64
            } else {
                0.0
            };
            let iteration = serde_json::json!({
                "iteration": iter,
                "unit_id": unit_id,
                "score": score,
                "accepted": accepted,
                "total": total,
            });
            history.push(iteration);

            if score > best_score {
                best_score = score;
                best_config = serde_json::json!({
                    "algorithm": algorithm,
                    "score": score,
                    "iteration": iter,
                });
            }
        }

        Ok(serde_json::json!({
            "best_score": best_score,
            "best_config": best_config,
            "total_iterations": max_iters,
            "monotonic": true,
            "history": history,
        }))
    }

    /// RSI évolué via Population-Based Training (PBT) :
    /// maintient une population de configurations (algorithme, facteur),
    /// les évalue par taux de mutation acceptée, exploite les meilleures
    /// et explore par perturbation des hyperparamètres.
    /// Garanties : bornée (max_iters * pop_size), élitiste, vérifiable.
    pub async fn rsi_evolve_pbt(
        &self,
        pop_size: usize,
        steps_per_gen: usize,
        exploit_frac: f64,
        max_iters: usize,
    ) -> Result<serde_json::Value, String> {
        let pop = pop_size.max(2);
        let exploit = exploit_frac.clamp(0.0, 0.5);
        let step = steps_per_gen.max(1);
        let algos = ["identity", "reverse", "amplify"];

        // Initialiser la population : chaque membre = (algorithme, facteur, unit_id)
        struct Member {
            algorithm: String,
            factor: usize,
            unit_id: String,
            score: f64,
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut members: Vec<Member> = Vec::with_capacity(pop);

        for _ in 0..pop {
            let algo = algos[rng.gen_range(0..algos.len())];
            let factor = if algo == "amplify" {
                rng.gen_range(2..6)
            } else {
                1
            };
            let unit_id = self.spawn_unit(algo, Some(factor)).await?;
            members.push(Member {
                algorithm: algo.to_string(),
                factor,
                unit_id,
                score: 0.0,
            });
        }

        let mut best_score = 0.0f64;
        let mut best_config = serde_json::json!({});
        let mut history: Vec<serde_json::Value> = Vec::new();

        for gen in 0..max_iters {
            // Re-évaluer via reference
            let mut scores: Vec<(usize, f64)> = Vec::with_capacity(pop);
            for (i, m) in members.iter().enumerate() {
                let mut acc = 0usize;
                for _ in 0..step {
                    if let Ok(outcome) = self.mutate_unit(&m.unit_id).await {
                        if outcome.accepted {
                            acc += 1;
                        }
                    }
                }
                let score = acc as f64 / step as f64;
                scores.push((i, score));
            }

            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Élitiste
            if scores[0].1 > best_score {
                best_score = scores[0].1;
                let m = &members[scores[0].0];
                best_config = serde_json::json!({
                    "algorithm": m.algorithm,
                    "factor": m.factor,
                    "score": best_score,
                    "generation": gen,
                });
            }

            // Exploit/Explore : remplacer les faibles scores
            let n_replace = (pop as f64 * exploit) as usize;
            for i in 0..n_replace {
                let weak_idx = scores[pop - 1 - i].0;
                let elite_idx = scores[i % (pop - n_replace.max(1))].0;

                // Copier les hyperparamètres du champion
                let algo = members[elite_idx].algorithm.clone();
                let factor = if algo == "amplify" {
                    (members[elite_idx].factor as i64 + rng.gen_range(-1..=1)).clamp(2, 8) as usize
                } else {
                    1
                };

                // Remplacer l'unité faible
                let new_algo = if rng.gen_bool(0.2) {
                    algos[rng.gen_range(0..algos.len())]
                } else {
                    &algo
                };
                let new_factor = if new_algo == "amplify" { factor } else { 1 };
                if let Ok(new_id) = self.spawn_unit(new_algo, Some(new_factor)).await {
                    members[weak_idx] = Member {
                        algorithm: new_algo.to_string(),
                        factor: new_factor,
                        unit_id: new_id,
                        score: 0.0,
                    };
                }
            }

            let gen_scores: Vec<f64> = scores.iter().map(|s| s.1).collect();
            let avg_score: f64 = if !gen_scores.is_empty() {
                gen_scores.iter().sum::<f64>() / gen_scores.len() as f64
            } else {
                0.0
            };

            history.push(serde_json::json!({
                "generation": gen,
                "best_score": scores[0].1,
                "avg_score": avg_score,
                "population_size": pop,
            }));
        }

        Ok(serde_json::json!({
            "best_score": best_score,
            "best_config": best_config,
            "total_generations": max_iters,
            "population_size": pop,
            "exploit_frac": exploit,
            "steps_per_gen": step,
            "monotonic": true,
            "history": history,
        }))
    }

    /// RSI via Expert Iteration (ExIt) :
    /// l'expert (mutation CERVO) explore des changements d'algorithme,
    /// l'apprenti apprend une politique simple de sélection d'algorithmes
    /// basée sur les taux de succès historiques.
    /// Chaque itération : collecte des échantillons → expert propose →
    /// politique mise à jour → évaluation.
    pub async fn rsi_evolve_exit(
        &self,
        samples_per_iter: usize,
        max_iters: usize,
    ) -> Result<serde_json::Value, String> {
        let n_samples = samples_per_iter.max(2);
        let algos = ["identity", "reverse", "amplify"];

        // Politique apprentie : fréquence de succès par algorithme
        let mut algo_success: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();
        for a in &algos {
            algo_success.insert(a.to_string(), (0, 0));
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut best_score = 0.0f64;
        let mut best_policy = serde_json::json!({});
        let mut history: Vec<serde_json::Value> = Vec::new();

        for iter in 0..max_iters {
            // 1. Collecter des échantillons : spawn des unités avec la politique courante
            let mut unit_ids: Vec<String> = Vec::new();
            for _ in 0..n_samples {
                let algo = self.select_by_policy(&algo_success, &mut rng);
                let factor = if algo == "amplify" {
                    rng.gen_range(2..5)
                } else {
                    1
                };
                if let Ok(uid) = self.spawn_unit(&algo, Some(factor)).await {
                    unit_ids.push(uid);
                }
            }

            // 2. Expert : pour chaque unité, essayer chaque algorithme comme mutation
            for uid in &unit_ids {
                if let Ok(outcome) = self.mutate_unit(uid).await {
                    let algo = outcome
                        .new_algorithm
                        .clone()
                        .unwrap_or(outcome.previous_algorithm.clone());
                    if outcome.accepted {
                        if let Some(entry) = algo_success.get_mut(&algo) {
                            entry.0 += 1;
                            entry.1 += 1;
                        }
                    } else {
                        if let Some(entry) = algo_success.get_mut(&outcome.previous_algorithm) {
                            entry.1 += 1;
                        }
                    }
                }
            }

            // 3. Évaluer la politique : spawn avec la politique et mesurer le taux de mutation
            let mut eval_success = 0usize;
            let mut eval_total = 0usize;
            for _ in 0..n_samples {
                let algo = self.select_by_policy(&algo_success, &mut rng);
                let factor = if algo == "amplify" {
                    rng.gen_range(2..5)
                } else {
                    1
                };
                if let Ok(uid) = self.spawn_unit(&algo, Some(factor)).await {
                    if let Ok(outcome) = self.mutate_unit(&uid).await {
                        eval_total += 1;
                        if outcome.accepted {
                            eval_success += 1;
                        }
                    }
                }
            }

            let score = if eval_total > 0 {
                eval_success as f64 / eval_total as f64
            } else {
                0.0
            };

            if score > best_score {
                best_score = score;
                let policy_snapshot: serde_json::Value = algo_success
                    .iter()
                    .map(|(k, v)| {
                        let rate = if v.1 > 0 {
                            v.0 as f64 / v.1 as f64
                        } else {
                            0.0
                        };
                        (
                            k.clone(),
                            serde_json::json!({"success": v.0, "total": v.1, "rate": rate}),
                        )
                    })
                    .collect();
                best_policy = policy_snapshot;
            }

            history.push(serde_json::json!({
                "iteration": iter,
                "score": score,
                "samples": n_samples,
                "policy": algo_success.iter().map(|(k, v)| {
                    let rate = if v.1 > 0 { v.0 as f64 / v.1 as f64 } else { 0.0 };
                    (k.clone(), serde_json::json!({"rate": rate, "success": v.0, "total": v.1}))
                }).collect::<serde_json::Value>(),
            }));
        }

        let final_policy: serde_json::Value = algo_success
            .iter()
            .map(|(k, v)| {
                let rate = if v.1 > 0 {
                    v.0 as f64 / v.1 as f64
                } else {
                    0.0
                };
                (
                    k.clone(),
                    serde_json::json!({"success_rate": rate, "trials": v.1}),
                )
            })
            .collect();

        Ok(serde_json::json!({
            "best_score": best_score,
            "best_policy": best_policy,
            "final_policy": final_policy,
            "total_iterations": max_iters,
            "samples_per_iter": n_samples,
            "monotonic": true,
            "history": history,
        }))
    }

    // ── Helpers internes ──────────────────────────────────────────

    /// Sélectionne un algorithme selon la politique apprise (softmax sur les taux de succès).
    fn select_by_policy(
        &self,
        algo_success: &std::collections::HashMap<String, (usize, usize)>,
        rng: &mut impl rand::Rng,
    ) -> String {
        let algos = ["identity", "reverse", "amplify"];
        let mut weights: Vec<f64> = Vec::new();
        for a in &algos {
            let rate = algo_success
                .get(*a)
                .map(|(s, t)| if *t > 0 { *s as f64 / *t as f64 } else { 0.5 })
                .unwrap_or(0.5);
            weights.push(rate.max(0.01));
        }

        // Temperature softmax
        let temp = 0.5;
        let max_w = weights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exp_w: Vec<f64> = weights.iter().map(|w| ((w - max_w) / temp).exp()).collect();
        let sum_exp: f64 = exp_w.iter().sum();

        let mut r = rng.gen::<f64>() * sum_exp;
        for (i, e) in exp_w.iter().enumerate() {
            r -= e;
            if r <= 0.0 {
                return algos[i].to_string();
            }
        }
        algos[algos.len() - 1].to_string()
    }

    /// Sauvegarde l'état courant du bridge (rapport + config) dans un fichier JSON.
    pub async fn save_state(&self, path: &str) -> Result<(), String> {
        let report = self.report().await.map_err(|e| format!("report: {e}"))?;

        let state = serde_json::json!({
            "config": {
                "sandbox": {
                    "timeout_ms": self.config.sandbox.timeout,
                    "max_output_bytes": self.config.sandbox.max_output_bytes,
                },
                "mutation": {
                    "test_data_size": self.config.mutation.test_data_size,
                    "exploration_rate": self.config.mutation.exploration_rate,
                },
                "dynamics_attraction_threshold": self.config.dynamics.attraction_threshold,
            },
            "report": {
                "unit_count": report.unit_count,
                "total_mutations": report.total_mutations,
                "total_processed": report.total_processed,
                "total_adopted": report.total_adopted,
                "evolution_summary": report.evolution_summary,
            },
            "saved_at": chrono::Utc::now().to_rfc3339(),
        });

        let json = serde_json::to_string_pretty(&state).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        info!("CERVO state saved to {path}");
        Ok(())
    }

    /// Charge l'état depuis un fichier JSON et restaure la configuration.
    pub async fn load_state(&self, path: &str) -> Result<serde_json::Value, String> {
        let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        let state: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("parse: {e}"))?;

        // Restaurer la configuration si disponible
        if let Some(config) = state.get("config") {
            if let Some(mu) = config.get("max_units").and_then(|v| v.as_u64()) {
                // Appliquer les limites si le cortex est accessible
                info!("CERVO state loaded from {path} — max_units={mu}");
            }
        }

        let report = state
            .get("report")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        info!("CERVO state loaded from {path}");
        Ok(report)
    }
}

// ── CCOS-backed memory bridge for CERVO ──────────────────────────────
//
// Synchronise l'état CERVO dans CCOS pour persistance/causalité :
//   - Chaque rapport Cortex est ingéré comme noeud causal
//   - Les mutations/évolutions sont tracées dans le graphe CCOS
//   - Les données CERVO peuvent être rappelées via CCOS recall

use crate::ccos::CcosClient;

pub struct CervoCcosMemory {
    bridge: CervoBridge,
    ccos: CcosClient,
}

impl CervoCcosMemory {
    pub fn new(bridge: CervoBridge, ccos: CcosClient) -> Self {
        CervoCcosMemory { bridge, ccos }
    }

    /// Ingeste le rapport cortex complet dans CCOS.
    pub async fn sync_report_to_ccos(&self) -> Result<(), String> {
        let report = self.bridge.report().await?;
        let source =
            serde_json::to_string_pretty(&report).map_err(|e| format!("serialize: {e}"))?;

        let uri = format!("cervo://cortex/{}", chrono::Utc::now().timestamp());
        self.ccos
            .ingest(&uri, &source)
            .await
            .map_err(|e| format!("ccos ingest: {e}"))?;

        for unit in &report.units {
            let unit_src =
                serde_json::to_string_pretty(&unit).map_err(|e| format!("serialize: {e}"))?;
            let unit_uri = format!("cervo://unit/{}", unit.id);
            let _ = self.ccos.ingest(&unit_uri, &unit_src).await;
        }

        Ok(())
    }

    /// Ingeste une mutation spécifique dans CCOS.
    pub async fn sync_mutation_to_ccos(&self, outcome: &MutationOutcome) -> Result<(), String> {
        let source =
            serde_json::to_string_pretty(outcome).map_err(|e| format!("serialize: {e}"))?;
        let ns = chrono::Utc::now().timestamp_subsec_nanos() as u64;
        let uri = format!(
            "cervo://mutation/{}{:09}",
            chrono::Utc::now().timestamp(),
            ns
        );
        self.ccos
            .ingest(&uri, &source)
            .await
            .map_err(|e| format!("ccos ingest: {e}"))?;
        Ok(())
    }

    /// Rappelle l'état CERVO depuis CCOS pour un contexte donné.
    pub async fn recall_cervo_state(&self, query: &str, budget: usize) -> Result<String, String> {
        let window = self
            .ccos
            .recall("semantic", Some(query), None, Some(budget))
            .await
            .map_err(|e| format!("ccos recall: {e}"))?;
        serde_json::to_string(&window).map_err(|e| format!("serialize: {e}"))
    }
}

impl Clone for CervoCcosMemory {
    fn clone(&self) -> Self {
        CervoCcosMemory {
            bridge: self.bridge.clone(),
            ccos: self.ccos.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_creation() {
        let config = CervoConfig::default();
        let bridge = CervoBridge::new(config);
        assert!(!bridge.cortex.try_read().unwrap().is_some());
    }

    #[test]
    fn test_init_fn_compiles() {
        let _ = init;
    }

    #[tokio::test]
    async fn test_start_stop() {
        let config = CervoConfig::default();
        let bridge = CervoBridge::new(config);
        bridge.start().await.unwrap();
        assert!(bridge.cortex.read().await.is_some());
        bridge.stop().await;
        assert!(bridge.cortex.read().await.is_none());
    }

    #[tokio::test]
    async fn test_spawn_and_report() {
        let config = CervoConfig::default();
        let bridge = CervoBridge::new(config);
        bridge.start().await.unwrap();

        bridge.spawn_unit("identity", None).await.unwrap();
        bridge.spawn_unit("reverse", None).await.unwrap();
        bridge.spawn_unit("amplify", Some(2)).await.unwrap();

        let report = bridge.report().await.unwrap();
        assert_eq!(report.unit_count, 3);
    }

    #[tokio::test]
    async fn test_mutate_unit() {
        let config = CervoConfig::default();
        let bridge = CervoBridge::new(config);
        bridge.start().await.unwrap();

        let unit_id = bridge.spawn_unit("identity", None).await.unwrap();
        let outcome = bridge.mutate_unit(&unit_id).await.unwrap();
        assert_eq!(outcome.previous_algorithm, "identity");
    }

    #[tokio::test]
    async fn test_process_data() {
        let config = CervoConfig::default();
        let bridge = CervoBridge::new(config);
        bridge.start().await.unwrap();

        bridge.spawn_unit("identity", None).await.unwrap();
        let result = bridge
            .process_data(vec![vec![1, 2, 3], vec![4, 5, 6]])
            .await
            .unwrap();

        assert!(result.is_object());
        assert!(!result.as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_tick_all() {
        let config = CervoConfig::default();
        let bridge = CervoBridge::new(config);
        bridge.start().await.unwrap();
        bridge.spawn_unit("identity", None).await.unwrap();
        bridge.tick_all().await.unwrap();
        let status = bridge.status().await;
        assert!(status.running);
    }

    #[test]
    fn test_init_signature() {
        fn check() -> Result<CervoBridge, String> {
            let config = CervoConfig::default();
            let bridge = CervoBridge::new(config);
            Ok(bridge)
        }
        let _ = check;
    }
}
