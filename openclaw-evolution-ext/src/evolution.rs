use crate::agent::Population;
use crate::audit::{AuditAction, AuditInput, Auditor, AuditVerdict};
use crate::batch_embed::BatchEmbedder;
use crate::concept::ConceptEngine;
use crate::concept_graph::LiveConceptGraph;
use crate::config::Config;
use crate::crawler::Crawler;
use crate::il::{ILExecutionResult, ILProgram, OpCode, StepStatus};
use crate::memory::Memory;
use crate::meta_language::MetaLanguage;
use crate::ollama::{InferenceMode, OllamaEngine};
use crate::organ_moe::OrganNetwork;
use crate::ranking::RankingEngine;
use crate::sandbox::Sandbox;
use crate::snapshot::{Snapshot, SnapshotManager};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{error, info, warn};

pub struct EvolutionSystem {
    pub config: Config,
    pub population: Population,
    pub memory: Memory,
    pub meta_language: MetaLanguage,
    pub concept_engine: ConceptEngine,
    pub il_version: u64,
    pub cycle: u64,
    crawler: Crawler,
    ranking: RankingEngine,
    ollama: OllamaEngine,
    sandbox: Sandbox,
    snapshot_mgr: SnapshotManager,
    // ── v0.3 : Nouveaux systèmes ─────────
    batch_embedder: BatchEmbedder,
    concept_graph: LiveConceptGraph,
    organ_network: OrganNetwork,
    auditor: Auditor,
    // Pipeline temporaire
    _last_crawl_results: Option<Vec<crate::crawler::CrawlResult>>,
    _last_ranked_context: Option<String>,
    /// Fitness du cycle précédent (pour l'audit)
    prev_avg_fitness: f32,
}

impl EvolutionSystem {
    pub async fn initialize(config: Config) -> Result<Self> {
        info!("═══════════════════════════════════════════");
        info!("  OPENCLAW v0.3 — Initialisation AUDITÉE");
        info!("═══════════════════════════════════════════");

        let ollama = OllamaEngine::new(
            &config.ollama_url,
            &config.model_fast,
            &config.model_deep,
            &config.model_embed,
            config.context_window_fast,
            config.context_window_deep,
            config.max_concurrent_fast,
            config.max_concurrent_deep,
        );

        match ollama.health_check().await {
            Ok(true) => info!("✓ Ollama connecté (fast={}, deep={}, embed={})",
                config.model_fast, config.model_deep, config.model_embed),
            _ => warn!("⚠ Ollama non disponible"),
        }

        let crawler = Crawler::new(config.http_timeout_secs, config.crawler_max_retries)?;
        let ranking = RankingEngine::new(config.max_crawl_results);
        let snapshot_mgr = SnapshotManager::new(config.snapshot_dir.clone(), 20)?;
        let sandbox = Sandbox::new(config.sandbox_dir.clone(), config.sandbox_timeout_secs)?;

        // v0.3 : Batch embedder (GPU accelerated)
        let batch_embedder = BatchEmbedder::new(
            &config.ollama_url, &config.model_embed, 1, 20,
        );

        // v0.3 : GNN Concept Graph vivant
        let concept_graph = LiveConceptGraph::new(384, 500); // nomic-embed = 768 dim, on prend 384

        // v0.3 : MoE Organ Network (18 organes)
        let organ_network = OrganNetwork::initialize();

        // v0.3 : Auditor
        let audit_dir = config.logs_dir.join("audit");
        let auditor = Auditor::new(audit_dir);

        let (population, memory, meta_language, concept_engine, il_version, cycle) =
            match snapshot_mgr.load_latest()? {
                Some(snap) => {
                    info!("◄ Restauration snapshot cycle={}", snap.cycle);
                    let memory = Memory::new(1000);
                    memory.import(snap.memory_data);
                    (snap.population, memory, snap.meta_language, snap.concept_engine,
                     snap.il_version, snap.cycle + 1)
                }
                None => {
                    info!("► Nouveau système");
                    (Population::initialize(config.population_size), Memory::new(1000),
                     MetaLanguage::new(), ConceptEngine::new(30), 0, 0)
                }
            };

        std::fs::create_dir_all(&config.agents_dir)?;
        std::fs::create_dir_all(&config.logs_dir)?;

        let net_stats = organ_network.stats();
        info!("Population: {} agents | Mémoire: {} | Concepts: {}",
            population.agents.len(), memory.len(), concept_engine.concepts.len());
        info!("MoE: {} organes, {} experts total | Graph: initialisé",
            net_stats.n_organs, net_stats.total_experts);
        info!("Audit: ACTIF (seuils chargés)");
        info!("═══════════════════════════════════════════");

        Ok(Self {
            config, population, memory, meta_language, concept_engine,
            il_version, cycle, crawler, ranking, ollama, sandbox, snapshot_mgr,
            batch_embedder, concept_graph, organ_network, auditor,
            _last_crawl_results: None, _last_ranked_context: None,
            prev_avg_fitness: 0.0,
        })
    }

    /// Exécuter UN cycle d'évolution complet avec audit
    pub async fn run_cycle(&mut self) -> Result<CycleReport> {
        info!("───── CYCLE {} ─────", self.cycle);

        // Sauvegarder la fitness avant le cycle (pour l'audit)
        self.prev_avg_fitness = self.population.average_fitness();

        // ── 1. Observer + détecter stagnation ─────
        let is_stagnating = self.population.is_stagnating(
            self.config.stagnation_threshold,
            self.config.stagnation_min_improvement,
        );
        if is_stagnating {
            warn!("⚡ STAGNATION — bascule DEEP");
            self.ollama.set_mode(InferenceMode::Deep);
        } else {
            self.ollama.set_mode(InferenceMode::Fast);
        }

        // ── 2. Stratégie IL ─────
        let rule = self.meta_language.select_rule(is_stagnating).clone();
        let il_program = self.meta_language.expand_to_program(&rule, &["default"]);
        info!("Stratégie: {} ({} steps, mode={})", rule.pattern, il_program.len(), self.ollama.current_mode);

        // ── 3. Crawler web ─────
        let crawl_results = self.crawler.crawl_batch(&self.config.seed_urls, 4).await;
        let crawl_count = crawl_results.len();
        for cr in &crawl_results {
            let key = format!("crawl:{}:{}", self.cycle, cr.url);
            self.memory.store(key, cr.content.clone(), 0.5);
        }

        // ── 4. Batch Embed + Ranking sémantique (GPU) ─────
        let curiosity = self.ollama.generate_curiosity_task(
            &format!("fitness={:.3}, gen={}, stagnation={}", self.population.average_fitness(), self.population.generation, is_stagnating)
        ).await.unwrap_or_else(|_| "rust optimization".into());

        // Embed query + tous les contenus crawlés en UN SEUL batch
        let mut texts_to_embed: Vec<String> = vec![curiosity.clone()];
        texts_to_embed.extend(crawl_results.iter().map(|cr| {
            format!("{} {}", cr.title, &cr.content[..cr.content.len().min(400)])
        }));

        let embeddings = self.batch_embedder.embed_batch(&texts_to_embed).await.unwrap_or_default();

        // Ranking par similarité sémantique
        let mut ranked_crawl: Vec<(usize, f32, String)> = Vec::new();
        if !embeddings.is_empty() {
            let query_emb = &embeddings[0];
            if !query_emb.is_empty() {
                self.ranking.set_query_embedding(query_emb.clone());
                for (i, emb) in embeddings[1..].iter().enumerate() {
                    if i < crawl_results.len() && !emb.is_empty() {
                        let sim = BatchEmbedder::cosine_similarity(query_emb, emb);
                        ranked_crawl.push((i, sim, crawl_results[i].content.clone()));
                    }
                }
                ranked_crawl.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            }
        }

        // ── 5. Alimenter le Concept Graph ─────
        let crawl_pairs: Vec<(String, String)> = crawl_results.iter()
            .map(|cr| (cr.url.clone(), cr.content.clone()))
            .collect();
        let graph_added = self.concept_graph
            .ingest_from_crawl(&crawl_pairs, &mut self.batch_embedder)
            .await
            .unwrap_or(0);
        self.concept_graph.ingest_from_memory(&self.memory, &mut self.batch_embedder)
            .await
            .ok();

        // GNN propagation
        self.concept_graph.propagate_gnn();

        // ── 6. MoE Organ Network — propager le stimulus ─────
        let global_embedding = self.concept_graph.global_embedding();
        if !global_embedding.is_empty() && self.organ_network.organs.len() > 0 {
            self.organ_network.propagate(0, &global_embedding);
        }

        // ── 7. Mutations LLM ─────
        let context = ranked_crawl.iter().take(3)
            .map(|(_, _, content)| content.chars().take(400).collect::<String>())
            .collect::<Vec<_>>().join("\n---\n");

        let weak_threshold = self.population.average_fitness() * 0.7;
        let targets: Vec<usize> = (0..self.population.agents.len())
            .filter(|i| {
                self.population.agents[*i].is_weak(weak_threshold)
                || self.population.agents[*i].is_stagnant(self.config.stagnation_threshold)
                || rand::random::<f32>() < self.config.mutation_rate
            })
            .collect();

        let mut mutations_generated = 0usize;
        let mut mutations_with_code = 0usize;
        for idx in &targets {
            let agent = &self.population.agents[*idx];
            match self.ollama.generate_mutation(agent, &context).await {
                Ok(mutation) => {
                    mutations_generated += 1;
                    if !mutation.code_patch.is_empty() {
                        mutations_with_code += 1;
                        // Ingérer la mutation dans le concept graph
                        if let Some(emb) = embeddings.first() {
                            self.concept_graph.ingest_mutation(&agent.id, &mutation.description, emb.clone());
                        }
                    }
                    self.population.agents[*idx].apply_mutation(&mutation);
                }
                Err(e) => {
                    warn!("Mutation échouée agent {}: {}", &agent.id[..8], e);
                }
            }
        }

        // ── 8. Sandbox : évaluation RÉELLE ─────
        let mut sandbox_compile = 0usize;
        let mut sandbox_run = 0usize;
        if self.config.sandbox_enabled {
            for i in 0..self.population.agents.len() {
                if self.population.agents[i].code.is_empty() {
                    continue;
                }
                let result = self.sandbox.evaluate(&self.population.agents[i].id, &self.population.agents[i].code).await;
                if result.compiled { sandbox_compile += 1; }
                if result.ran_ok { sandbox_run += 1; }
                self.population.agents[i].update_from_sandbox(&result);
            }
        }

        // ── 9. Sélection naturelle ─────
        self.population.natural_selection(self.config.survival_rate);
        let new_avg = self.population.average_fitness();

        // ── 10. Évoluer méta-systèmes ─────
        self.meta_language.report_execution(&rule.pattern, new_avg > self.prev_avg_fitness);
        self.meta_language.evolve_grammar();
        self.concept_engine.evolve_all(new_avg);
        self.il_version += 1;

        // ── 11. AUDIT OBLIGATOIRE ─────
        let unique_codes: HashSet<String> = self.population.agents.iter()
            .filter(|a| !a.code.is_empty())
            .map(|a| {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(a.code.as_bytes());
                hex::encode(h.finalize())
            })
            .collect();

        let avg_code_lines = if unique_codes.is_empty() {
            0.0
        } else {
            let total_lines: usize = self.population.agents.iter()
                .filter(|a| !a.code.is_empty())
                .map(|a| a.code.lines().count())
                .sum();
            total_lines as f32 / unique_codes.len() as f32
        };

        let graph_stats = self.concept_graph.stats();
        let embed_stats = self.batch_embedder.stats();
        let net_stats = self.organ_network.stats();

        let audit_input = AuditInput {
            cycle: self.cycle,
            mutations_generated,
            mutations_with_code,
            agents_compiling: sandbox_compile,
            agents_running: sandbox_run,
            population_size: self.population.agents.len(),
            avg_fitness: new_avg,
            prev_avg_fitness: self.prev_avg_fitness,
            best_fitness: self.population.best_agent().map(|a| a.fitness).unwrap_or(0.0),
            unique_code_hashes: unique_codes.len(),
            avg_code_lines,
            graph_active_nodes: graph_stats.active_nodes,
            graph_strong_edges: graph_stats.strong_edges,
            embeddings_generated: embed_stats.total_embedded,
            moe_organs_active: net_stats.n_organs,
            inference_mode: format!("{}", self.ollama.current_mode),
        };

        let audit_report = self.auditor.audit(&audit_input);

        // ── 12. Appliquer les actions de l'audit ─────
        match audit_report.recommended_action {
            AuditAction::Continue => { /* rien */ }
            AuditAction::IncreaseMutation => {
                warn!("AUDIT → Augmentation taux de mutation: {:.0}% → {:.0}%",
                    self.config.mutation_rate * 100.0, (self.config.mutation_rate * 1.5).min(0.8) * 100.0);
                self.config.mutation_rate = (self.config.mutation_rate * 1.5).min(0.8);
            }
            AuditAction::SwitchToDeep => {
                warn!("AUDIT → Bascule forcée en mode DEEP");
                self.ollama.set_mode(InferenceMode::Deep);
            }
            AuditAction::ForceRollback => {
                error!("AUDIT → ROLLBACK FORCÉ ({} échecs consécutifs)", audit_report.consecutive_failures);
                if let Ok(Some(snap)) = self.snapshot_mgr.load_latest() {
                    self.restore_from_snapshot(snap);
                    info!("Restauré depuis dernier snapshot");
                }
            }
            AuditAction::CriticalAlert => {
                error!("AUDIT → ALERTE CRITIQUE — intervention humaine recommandée");
            }
        }

        // ── 13. Snapshot ─────
        if self.cycle % self.config.snapshot_interval as u64 == 0 {
            let snap = SnapshotManager::create_snapshot(
                self.cycle, &self.population, self.memory.export(),
                self.il_version, &self.meta_language, &self.concept_engine,
                self.ollama.current_mode,
            );
            self.snapshot_mgr.save(&snap).ok();
        }

        let report = CycleReport {
            cycle: self.cycle,
            avg_fitness: new_avg,
            best_fitness: self.population.best_agent().map(|a| a.fitness).unwrap_or(0.0),
            agents_compiling: sandbox_compile,
            agents_running: sandbox_run,
            mutations_generated,
            mutations_with_code,
            unique_codes: unique_codes.len(),
            population_size: self.population.agents.len(),
            memory_entries: self.memory.len(),
            graph_nodes: graph_stats.active_nodes,
            graph_edges: graph_stats.strong_edges,
            moe_organs: net_stats.n_organs,
            embeddings_batched: embed_stats.total_embedded,
            generation: self.population.generation,
            inference_mode: format!("{}", self.ollama.current_mode),
            il_strategy: rule.pattern.clone(),
            audit_verdict: format!("{}", audit_report.overall_verdict),
            audit_action: format!("{}", audit_report.recommended_action),
        };

        info!(
            "CYCLE {} FIN — fitness={:.3} compile={}/{} run={}/{} codes={} audit={} graph={}n/{}e",
            self.cycle, new_avg, sandbox_compile, self.population.agents.len(),
            sandbox_run, self.population.agents.len(), unique_codes.len(),
            audit_report.overall_verdict, graph_stats.active_nodes, graph_stats.strong_edges
        );

        self.cycle += 1;
        Ok(report)
    }

    /// Boucle infinie
    pub async fn run_forever(&mut self) -> Result<()> {
        info!("╔══════════════════════════════════════════╗");
        info!("║  OPENCLAW v0.3 — Évolution AUDITÉE       ║");
        info!("╚══════════════════════════════════════════╝");

        loop {
            match self.run_cycle().await {
                Ok(report) => {
                    info!(
                        "C{}: fit={:.3} comp={}/{} codes={} audit={} mode={}",
                        report.cycle, report.avg_fitness,
                        report.agents_compiling, report.population_size,
                        report.unique_codes, report.audit_verdict, report.inference_mode
                    );
                }
                Err(e) => {
                    error!("Erreur cycle {}: {}", self.cycle, e);
                    if let Ok(Some(snap)) = self.snapshot_mgr.load_latest() {
                        self.restore_from_snapshot(snap);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(self.config.cycle_delay_secs)).await;
        }
    }

    fn restore_from_snapshot(&mut self, snap: Snapshot) {
        self.population = snap.population;
        self.memory.import(snap.memory_data);
        self.meta_language = snap.meta_language;
        self.concept_engine = snap.concept_engine;
        self.il_version = snap.il_version;
        self.cycle = snap.cycle + 1;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleReport {
    pub cycle: u64,
    pub avg_fitness: f32,
    pub best_fitness: f32,
    pub agents_compiling: usize,
    pub agents_running: usize,
    pub mutations_generated: usize,
    pub mutations_with_code: usize,
    pub unique_codes: usize,
    pub population_size: usize,
    pub memory_entries: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub moe_organs: usize,
    pub embeddings_batched: u64,
    pub generation: u64,
    pub inference_mode: String,
    pub il_strategy: String,
    pub audit_verdict: String,
    pub audit_action: String,
}
