//! # Concept Graph Vivant
//!
//! Graphe GNN de concepts interconnectés, alimenté par :
//! - la mémoire (DashMap) du système
//! - les résultats de crawl (wiki, web)
//! - les mutations d'agents (code produit)
//!
//! Le graphe évolue : les nœuds se renforcent par l'usage,
//! les arêtes se créent par co-occurrence, le GNN propage
//! les embeddings pour aligner la sémantique.

use crate::batch_embed::BatchEmbedder;
use crate::math::graph::{ConceptGraph, GNNStack, graph_readout, Node, Edge};
use crate::memory::Memory;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

// ─────────────────────────────────────────────
// Concept node enrichi
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptNode {
    pub id: usize,
    pub label: String,
    /// Embedding sémantique
    pub embedding: Vec<f32>,
    /// Nombre d'accès (renforcement par l'usage)
    pub access_count: u64,
    /// Score de pertinence courant
    pub relevance: f32,
    /// Source : "memory", "crawl", "mutation", "wiki"
    pub source: String,
    /// Timestamp de dernière mise à jour
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptEdge {
    pub src: usize,
    pub dst: usize,
    /// Force de la connexion (co-occurrence pondérée)
    pub strength: f32,
    /// Nombre de co-occurrences
    pub co_occurrences: u64,
}

// ─────────────────────────────────────────────
// Graphe de concepts vivant
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
pub struct LiveConceptGraph {
    pub nodes: Vec<ConceptNode>,
    pub edges: Vec<ConceptEdge>,
    /// Index label → node_id
    label_index: HashMap<String, usize>,
    /// Dimension des embeddings
    pub embed_dim: usize,
    /// Max nodes
    pub max_nodes: usize,
    /// GNN hidden dim
    pub gnn_dim: usize,
    /// Nombre de couches GNN
    pub gnn_layers: usize,
}

impl LiveConceptGraph {
    pub fn new(embed_dim: usize, max_nodes: usize) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            label_index: HashMap::new(),
            embed_dim,
            max_nodes,
            gnn_dim: embed_dim,
            gnn_layers: 3,
        }
    }

    /// Ajouter ou mettre à jour un concept
    pub fn upsert_concept(&mut self, label: &str, embedding: Vec<f32>, source: &str) -> usize {
        if let Some(&id) = self.label_index.get(label) {
            // Mise à jour : moyenne mobile de l'embedding
            let node = &mut self.nodes[id];
            let alpha = 0.3;
            for i in 0..node.embedding.len().min(embedding.len()) {
                node.embedding[i] = node.embedding[i] * (1.0 - alpha) + embedding[i] * alpha;
            }
            node.access_count += 1;
            node.relevance = (node.relevance * 0.9) + 0.1;
            node.updated_at = chrono::Utc::now();
            id
        } else {
            // Nouveau concept
            let id = self.nodes.len();

            // Élaguer si plein
            if self.nodes.len() >= self.max_nodes {
                self.prune_weakest();
            }

            self.nodes.push(ConceptNode {
                id,
                label: label.to_string(),
                embedding,
                access_count: 1,
                relevance: 0.5,
                source: source.to_string(),
                updated_at: chrono::Utc::now(),
            });
            self.label_index.insert(label.to_string(), id);
            id
        }
    }

    /// Ajouter ou renforcer une arête entre deux concepts
    pub fn connect(&mut self, a: usize, b: usize, strength_delta: f32) {
        if a == b || a >= self.nodes.len() || b >= self.nodes.len() {
            return;
        }

        // Chercher l'arête existante
        for edge in &mut self.edges {
            if (edge.src == a && edge.dst == b) || (edge.src == b && edge.dst == a) {
                edge.strength = (edge.strength + strength_delta).min(1.0);
                edge.co_occurrences += 1;
                return;
            }
        }

        // Nouvelle arête
        self.edges.push(ConceptEdge {
            src: a,
            dst: b,
            strength: strength_delta.min(1.0),
            co_occurrences: 1,
        });
    }

    /// Ingérer des connaissances depuis la mémoire du système
    pub async fn ingest_from_memory(
        &mut self,
        memory: &Memory,
        embedder: &mut BatchEmbedder,
    ) -> Result<usize> {
        let top_entries = memory.top_entries(50);
        if top_entries.is_empty() {
            return Ok(0);
        }

        // Extraire les labels (clés mémoire) et contenus
        let labels: Vec<String> = top_entries.iter().map(|(k, _)| k.clone()).collect();
        let contents: Vec<String> = top_entries
            .iter()
            .map(|(_, e)| e.value.chars().take(500).collect())
            .collect();

        // Batch embed de tous les contenus
        let embeddings = embedder.embed_batch(&contents).await?;

        let mut added = 0;
        for (i, (label, _entry)) in top_entries.iter().enumerate() {
            if i < embeddings.len() && !embeddings[i].is_empty() {
                // Extraire des mots-clés du label comme concept
                let concept_label = extract_concept_label(label);
                let id = self.upsert_concept(&concept_label, embeddings[i].clone(), "memory");
                added += 1;

                // Connecter les concepts co-localisés
                if i > 0 {
                    let prev_label = extract_concept_label(&labels[i - 1]);
                    if let Some(&prev_id) = self.label_index.get(&prev_label) {
                        self.connect(id, prev_id, 0.2);
                    }
                }
            }
        }

        info!(
            "Concept graph: ingéré {} concepts depuis mémoire ({} nœuds total)",
            added,
            self.nodes.len()
        );
        Ok(added)
    }

    /// Ingérer des résultats de crawl
    pub async fn ingest_from_crawl(
        &mut self,
        crawl_texts: &[(String, String)], // (url, content)
        embedder: &mut BatchEmbedder,
    ) -> Result<usize> {
        if crawl_texts.is_empty() {
            return Ok(0);
        }

        let contents: Vec<String> = crawl_texts
            .iter()
            .map(|(_, c)| c.chars().take(500).collect())
            .collect();
        let embeddings = embedder.embed_batch(&contents).await?;

        let mut added = 0;
        let mut ids: Vec<usize> = Vec::new();

        for (i, (url, _)) in crawl_texts.iter().enumerate() {
            if i < embeddings.len() && !embeddings[i].is_empty() {
                let label = extract_concept_label(url);
                let id = self.upsert_concept(&label, embeddings[i].clone(), "crawl");
                ids.push(id);
                added += 1;
            }
        }

        // Connecter les concepts du même batch de crawl
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len().min(i + 4) {
                self.connect(ids[i], ids[j], 0.15);
            }
        }

        Ok(added)
    }

    /// Ingérer une mutation d'agent (code produit)
    pub fn ingest_mutation(&mut self, agent_id: &str, mutation_desc: &str, embedding: Vec<f32>) {
        let label = format!("mutation:{}", &agent_id[..8.min(agent_id.len())]);
        let id = self.upsert_concept(&label, embedding, "mutation");

        // Connecter à l'agent comme concept
        let agent_label = format!("agent:{}", &agent_id[..8.min(agent_id.len())]);
        if let Some(&agent_id) = self.label_index.get(&agent_label) {
            self.connect(id, agent_id, 0.3);
        }
    }

    /// Propager les embeddings via GNN (message passing)
    pub fn propagate_gnn(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        // Construire le graphe math::graph::ConceptGraph
        let mut gnn_graph = ConceptGraph::new();

        for node in &self.nodes {
            let padded = pad_to(& node.embedding, self.gnn_dim);
            gnn_graph.add_node(&node.label, padded);
        }

        for edge in &self.edges {
            if edge.src < gnn_graph.n_nodes() && edge.dst < gnn_graph.n_nodes() {
                gnn_graph.add_edge(edge.src, edge.dst, edge.strength);
            }
        }

        // Propager via GNN stack
        let stack = GNNStack::new(self.gnn_dim, self.gnn_layers);
        stack.forward(&mut gnn_graph);

        // Mettre à jour les embeddings des nœuds depuis le GNN
        for (i, gnn_node) in gnn_graph.nodes.iter().enumerate() {
            if i < self.nodes.len() {
                // Moyenne mobile entre l'ancien embedding et le résultat GNN
                let alpha = 0.2;
                for d in 0..self.nodes[i].embedding.len().min(gnn_node.h.len()) {
                    self.nodes[i].embedding[d] =
                        self.nodes[i].embedding[d] * (1.0 - alpha) + gnn_node.h[d] * alpha;
                }
            }
        }

        info!("GNN propagation: {} nœuds, {} arêtes", self.nodes.len(), self.edges.len());
    }

    /// Trouver les concepts les plus proches d'un embedding
    pub fn nearest_concepts(&self, query_embedding: &[f32], top_k: usize) -> Vec<(usize, f32)> {
        let mut scores: Vec<(usize, f32)> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let sim = cosine_sim(&node.embedding, query_embedding);
                (i, sim)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(top_k);
        scores
    }

    /// Embedding global du graphe (readout)
    pub fn global_embedding(&self) -> Vec<f32> {
        if self.nodes.is_empty() {
            return vec![0.0; self.embed_dim];
        }

        let n = self.nodes.len() as f32;
        let mut global = vec![0.0; self.embed_dim];
        for node in &self.nodes {
            let weight = node.relevance;
            for d in 0..self.embed_dim.min(node.embedding.len()) {
                global[d] += node.embedding[d] * weight;
            }
        }
        let total_weight: f32 = self.nodes.iter().map(|n| n.relevance).sum();
        if total_weight > 0.0 {
            global.iter_mut().for_each(|x| *x /= total_weight);
        }
        global
    }

    /// Élaguer les concepts les moins pertinents
    fn prune_weakest(&mut self) {
        if self.nodes.len() < 2 {
            return;
        }

        // Trouver le nœud le moins pertinent
        let weakest = self
            .nodes
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.relevance.partial_cmp(&b.1.relevance).unwrap())
            .map(|(i, _)| i);

        if let Some(idx) = weakest {
            let label = self.nodes[idx].label.clone();
            self.label_index.remove(&label);
            self.edges.retain(|e| e.src != idx && e.dst != idx);
            // Decay relevance au lieu de supprimer (garder les indices stables)
            self.nodes[idx].relevance = 0.0;
            self.nodes[idx].label = format!("_pruned_{}", idx);
        }
    }

    /// Statistiques du graphe
    pub fn stats(&self) -> GraphStats {
        let active_nodes = self.nodes.iter().filter(|n| n.relevance > 0.01).count();
        let strong_edges = self.edges.iter().filter(|e| e.strength > 0.1).count();

        GraphStats {
            total_nodes: self.nodes.len(),
            active_nodes,
            total_edges: self.edges.len(),
            strong_edges,
            avg_relevance: if active_nodes > 0 {
                self.nodes.iter().map(|n| n.relevance).sum::<f32>() / active_nodes as f32
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub active_nodes: usize,
    pub total_edges: usize,
    pub strong_edges: usize,
    pub avg_relevance: f32,
}

// ── Helpers ─────────────────────────────────

fn extract_concept_label(key: &str) -> String {
    // Extraire un label propre d'une clé mémoire ou URL
    let cleaned = key
        .replace("crawl:", "")
        .replace("ranked:", "")
        .replace("https://", "")
        .replace("http://", "");
    // Garder les 50 premiers chars
    cleaned.chars().take(50).collect()
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 { return 0.0; }
    let dot: f32 = a[..len].iter().zip(b[..len].iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
    if na < 1e-10 || nb < 1e-10 { 0.0 } else { dot / (na * nb) }
}

fn pad_to(v: &[f32], target: usize) -> Vec<f32> {
    let mut padded = v.to_vec();
    padded.resize(target, 0.0);
    padded.truncate(target);
    padded
}
