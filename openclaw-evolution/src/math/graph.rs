//! # GNN Message Passing : h_v = σ(Σ_{u∈N(v)} W h_u)
//!
//! Traversée du graphe de concepts (mémoire sémantique).
//! Chaque nœud agrège les messages de ses voisins et met à jour son embedding.

use rand::Rng;
use serde::{Deserialize, Serialize};

/// Nœud du graphe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: usize,
    pub label: String,
    /// Embedding du nœud (hidden state)
    pub h: Vec<f32>,
    /// Features initiales
    pub x: Vec<f32>,
}

/// Arête du graphe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub src: usize,
    pub dst: usize,
    /// Poids de l'arête (optionnel)
    pub weight: f32,
}

/// Graphe de concepts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Liste d'adjacence (node_id → voisins)
    adjacency: Vec<Vec<(usize, f32)>>,
}

impl ConceptGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            adjacency: Vec::new(),
        }
    }

    /// Ajouter un nœud
    pub fn add_node(&mut self, label: &str, features: Vec<f32>) -> usize {
        let id = self.nodes.len();
        let h = features.clone();
        self.nodes.push(Node {
            id,
            label: label.to_string(),
            h,
            x: features,
        });
        self.adjacency.push(Vec::new());
        id
    }

    /// Ajouter une arête (bidirectionnelle)
    pub fn add_edge(&mut self, src: usize, dst: usize, weight: f32) {
        self.edges.push(Edge { src, dst, weight });
        self.adjacency[src].push((dst, weight));
        self.adjacency[dst].push((src, weight));
    }

    /// Voisins d'un nœud
    pub fn neighbors(&self, node_id: usize) -> &[(usize, f32)] {
        &self.adjacency[node_id]
    }

    /// Nombre de nœuds
    pub fn n_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Nombre d'arêtes
    pub fn n_edges(&self) -> usize {
        self.edges.len()
    }
}

/// Couche GNN avec message passing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GNNLayer {
    /// Matrice de poids W (hidden_dim × hidden_dim)
    pub weights: Vec<Vec<f32>>,
    /// Biais
    pub bias: Vec<f32>,
    /// Dimension cachée
    pub hidden_dim: usize,
    /// Type d'agrégation
    pub aggregation: AggregationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationType {
    /// Somme des messages
    Sum,
    /// Moyenne des messages
    Mean,
    /// Maximum des messages
    Max,
}

impl GNNLayer {
    pub fn new(hidden_dim: usize, aggregation: AggregationType) -> Self {
        let scale = (2.0 / hidden_dim as f32).sqrt();
        let mut rng = rand::thread_rng();

        Self {
            weights: (0..hidden_dim)
                .map(|_| {
                    (0..hidden_dim)
                        .map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * scale)
                        .collect()
                })
                .collect(),
            bias: vec![0.0; hidden_dim],
            hidden_dim,
            aggregation,
        }
    }

    /// Un pas de message passing sur tout le graphe
    ///
    /// Pour chaque nœud v :
    ///   m_v = AGG({W h_u : u ∈ N(v)})
    ///   h_v' = σ(m_v + b)
    pub fn forward(&self, graph: &mut ConceptGraph) {
        let n = graph.n_nodes();
        let mut new_h: Vec<Vec<f32>> = Vec::with_capacity(n);

        for v in 0..n {
            let neighbors = graph.neighbors(v).to_vec();

            if neighbors.is_empty() {
                // Pas de voisins → garder le hidden state + self-loop
                new_h.push(self.transform(&graph.nodes[v].h));
                continue;
            }

            // Phase 1 : Message — transformer les embeddings des voisins
            let messages: Vec<(Vec<f32>, f32)> = neighbors
                .iter()
                .map(|&(u, w)| (self.transform(&graph.nodes[u].h), w))
                .collect();

            // Phase 2 : Agrégation
            let aggregated = self.aggregate(&messages);

            // Phase 3 : Mise à jour avec activation ReLU
            let mut h_new = vec![0.0; self.hidden_dim];
            for d in 0..self.hidden_dim {
                h_new[d] = (aggregated[d] + self.bias[d]).max(0.0); // ReLU
            }

            new_h.push(h_new);
        }

        // Appliquer les nouveaux hidden states
        for (v, h) in new_h.into_iter().enumerate() {
            graph.nodes[v].h = h;
        }
    }

    /// Transformation linéaire : W @ h
    fn transform(&self, h: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; self.hidden_dim];
        for j in 0..self.hidden_dim {
            for i in 0..h.len().min(self.hidden_dim) {
                out[j] += self.weights[i][j] * h[i];
            }
        }
        out
    }

    /// Agrégation des messages selon le type choisi
    fn aggregate(&self, messages: &[(Vec<f32>, f32)]) -> Vec<f32> {
        let d = self.hidden_dim;

        match self.aggregation {
            AggregationType::Sum => {
                let mut agg = vec![0.0; d];
                for (msg, weight) in messages {
                    for j in 0..d {
                        agg[j] += msg[j] * weight;
                    }
                }
                agg
            }
            AggregationType::Mean => {
                let mut agg = vec![0.0; d];
                let total_weight: f32 = messages.iter().map(|(_, w)| w).sum();
                for (msg, weight) in messages {
                    for j in 0..d {
                        agg[j] += msg[j] * weight;
                    }
                }
                if total_weight > 1e-10 {
                    agg.iter_mut().for_each(|x| *x /= total_weight);
                }
                agg
            }
            AggregationType::Max => {
                let mut agg = vec![f32::NEG_INFINITY; d];
                for (msg, _) in messages {
                    for j in 0..d {
                        agg[j] = agg[j].max(msg[j]);
                    }
                }
                // Remplacer les -inf par 0
                agg.iter_mut().for_each(|x| {
                    if *x == f32::NEG_INFINITY {
                        *x = 0.0
                    }
                });
                agg
            }
        }
    }
}

/// Pile de couches GNN
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GNNStack {
    pub layers: Vec<GNNLayer>,
}

impl GNNStack {
    /// Créer une pile de n_layers couches
    pub fn new(hidden_dim: usize, n_layers: usize) -> Self {
        Self {
            layers: (0..n_layers)
                .map(|_| GNNLayer::new(hidden_dim, AggregationType::Mean))
                .collect(),
        }
    }

    /// Forward complet sur le graphe (n passes de message passing)
    pub fn forward(&self, graph: &mut ConceptGraph) {
        for layer in &self.layers {
            layer.forward(graph);
        }
    }
}

/// Readout : agréger les embeddings des nœuds en un vecteur global
pub fn graph_readout(graph: &ConceptGraph, mode: &str) -> Vec<f32> {
    if graph.nodes.is_empty() {
        return Vec::new();
    }

    let dim = graph.nodes[0].h.len();

    match mode {
        "mean" => {
            let mut agg = vec![0.0; dim];
            for node in &graph.nodes {
                for d in 0..dim {
                    agg[d] += node.h[d];
                }
            }
            let n = graph.nodes.len() as f32;
            agg.iter_mut().for_each(|x| *x /= n);
            agg
        }
        "sum" => {
            let mut agg = vec![0.0; dim];
            for node in &graph.nodes {
                for d in 0..dim {
                    agg[d] += node.h[d];
                }
            }
            agg
        }
        "max" => {
            let mut agg = vec![f32::NEG_INFINITY; dim];
            for node in &graph.nodes {
                for d in 0..dim {
                    agg[d] = agg[d].max(node.h[d]);
                }
            }
            agg
        }
        _ => graph_readout(graph, "mean"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_passing() {
        let mut graph = ConceptGraph::new();
        let a = graph.add_node("evolution", vec![1.0, 0.0, 0.0, 0.0]);
        let b = graph.add_node("optimization", vec![0.0, 1.0, 0.0, 0.0]);
        let c = graph.add_node("stability", vec![0.0, 0.0, 1.0, 0.0]);
        graph.add_edge(a, b, 1.0);
        graph.add_edge(b, c, 1.0);
        graph.add_edge(a, c, 0.5);

        let layer = GNNLayer::new(4, AggregationType::Mean);
        layer.forward(&mut graph);

        // Après message passing, les embeddings devraient avoir changé
        assert_ne!(graph.nodes[0].h, vec![1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_gnn_stack() {
        let mut graph = ConceptGraph::new();
        for i in 0..5 {
            graph.add_node(&format!("node_{}", i), vec![i as f32; 8]);
        }
        for i in 0..4 {
            graph.add_edge(i, i + 1, 1.0);
        }

        let stack = GNNStack::new(8, 3);
        stack.forward(&mut graph);

        let readout = graph_readout(&graph, "mean");
        assert_eq!(readout.len(), 8);
    }
}
