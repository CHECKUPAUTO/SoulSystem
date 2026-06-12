//! # Organ MoE — Mixture of Experts par organe SoulLink
//!
//! Chaque organe possède N experts (bassins d'attraction).
//! Le gating route chaque input vers top-k experts.
//! 18 organes × 8 experts = 144 sous-personnalités.
//!
//! Le routing s'adapte au contexte : un même organe peut répondre
//! différemment selon le type de stimulus.

use crate::math::moe::{MixtureOfExperts, MoEConfig, MoEOutput};
use crate::math::attention::softmax;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

// ─────────────────────────────────────────────
// Organe avec MoE intégré
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganType {
    pub name: String,
    pub port: u16,
    pub role: OrganRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrganRole {
    Science,
    Mind,
    Engineer,
    Crypto,
    Creative,
    Meta,
    Memory,
    Reflex,
    Integration,
    Perception,
    Affect,
    Language,
    Reasoning,
    Foresight,
    Homeostasis,
    Creativity,
    Social,
    Validation,
}

impl OrganRole {
    /// Dimension d'état recommandée pour cet organe
    pub fn state_dim(&self) -> usize {
        match self {
            OrganRole::Memory | OrganRole::Reasoning => 512,
            OrganRole::Foresight | OrganRole::Creativity => 256,
            _ => 128,
        }
    }

    /// Nombre d'experts recommandé
    pub fn n_experts(&self) -> usize {
        match self {
            OrganRole::Reasoning | OrganRole::Foresight => 8,
            OrganRole::Memory | OrganRole::Creativity | OrganRole::Social => 6,
            _ => 4,
        }
    }

    /// Tous les rôles
    pub fn all() -> Vec<OrganRole> {
        vec![
            OrganRole::Science, OrganRole::Mind, OrganRole::Engineer,
            OrganRole::Crypto, OrganRole::Creative, OrganRole::Meta,
            OrganRole::Memory, OrganRole::Reflex, OrganRole::Integration,
            OrganRole::Perception, OrganRole::Affect, OrganRole::Language,
            OrganRole::Reasoning, OrganRole::Foresight, OrganRole::Homeostasis,
            OrganRole::Creativity, OrganRole::Social, OrganRole::Validation,
        ]
    }
}

/// Organe MoE : un organe SoulLink avec mixture of experts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganMoE {
    pub organ: OrganType,
    pub moe: MixtureOfExperts,
    /// État courant de l'organe
    pub state: Vec<f32>,
    /// Historique de routing (quel expert activé quand)
    pub routing_history: Vec<Vec<(usize, f32)>>,
    /// Nombre max d'entrées dans l'historique
    pub max_history: usize,
}

impl OrganMoE {
    pub fn new(name: &str, port: u16, role: OrganRole) -> Self {
        let dim = role.state_dim();
        let n_experts = role.n_experts();

        let moe_config = MoEConfig {
            input_dim: dim,
            output_dim: dim,
            n_experts,
            top_k: 2,
            balance_coeff: 0.01,
        };

        Self {
            organ: OrganType {
                name: name.to_string(),
                port,
                role: role.clone(),
            },
            moe: MixtureOfExperts::new(moe_config),
            state: vec![0.0; dim],
            routing_history: Vec::new(),
            max_history: 100,
        }
    }

    /// Traiter un stimulus à travers le MoE
    pub fn process(&mut self, input: &[f32]) -> MoEOutput {
        let output = self.moe.forward(input);

        // Enregistrer le routing
        let routing: Vec<(usize, f32)> = output
            .expert_outputs
            .iter()
            .map(|(idx, weight, _)| (*idx, *weight))
            .collect();
        self.routing_history.push(routing);

        // Élaguer l'historique
        if self.routing_history.len() > self.max_history {
            self.routing_history
                .drain(0..self.routing_history.len() - self.max_history);
        }

        // Mettre à jour l'état
        self.state = output.output.clone();

        output
    }

    /// Expert dominant (le plus utilisé récemment)
    pub fn dominant_expert(&self) -> Option<(usize, f32)> {
        if self.routing_history.is_empty() {
            return None;
        }

        let mut counts: HashMap<usize, f32> = HashMap::new();
        let window = self.routing_history.len().min(20);
        let recent = &self.routing_history[self.routing_history.len() - window..];

        for routing in recent {
            for &(expert_idx, weight) in routing {
                *counts.entry(expert_idx).or_insert(0.0) += weight;
            }
        }

        counts
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }

    /// Distribution d'utilisation des experts
    pub fn usage_distribution(&self) -> Vec<f32> {
        self.moe.usage_distribution()
    }

    /// Reset les compteurs pour la prochaine période
    pub fn reset_usage(&mut self) {
        self.moe.reset_usage();
    }
}

// ─────────────────────────────────────────────
// Réseau d'organes MoE interconnectés
// ─────────────────────────────────────────────

/// Réseau complet d'organes SoulLink avec MoE
#[derive(Clone, Serialize, Deserialize)]
pub struct OrganNetwork {
    pub organs: Vec<OrganMoE>,
    /// Matrice de connexion entre organes (poids d'influence)
    pub connections: Vec<Vec<f32>>,
    /// Cycle global
    pub tick: u64,
}

impl OrganNetwork {
    /// Initialiser le réseau complet des 18 organes
    pub fn initialize() -> Self {
        let roles = OrganRole::all();
        let base_port = 9030u16;

        let organs: Vec<OrganMoE> = roles
            .iter()
            .enumerate()
            .map(|(i, role)| {
                OrganMoE::new(&format!("{:?}", role).to_lowercase(), base_port + i as u16, role.clone())
            })
            .collect();

        let n = organs.len();
        // Connexions initiales : tout-à-tout avec poids faible
        let connections = vec![vec![0.1; n]; n];

        Self {
            organs,
            connections,
            tick: 0,
        }
    }

    /// Propager un stimulus à travers tout le réseau
    /// Le stimulus entre par un organe source et se propage aux connectés
    pub fn propagate(&mut self, source_idx: usize, input: &[f32]) -> NetworkPropagation {
        let n = self.organs.len();
        let mut activations: Vec<Option<Vec<f32>>> = vec![None; n];
        let mut balance_loss_total = 0.0f32;

        // Phase 1 : activer l'organe source
        let dim = self.organs[source_idx].organ.role.state_dim();
        let padded = pad_or_truncate(input, dim);
        let output = self.organs[source_idx].process(&padded);
        activations[source_idx] = Some(output.output.clone());
        balance_loss_total += output.balance_loss;

        // Phase 2 : propager aux voisins (1 hop)
        for target_idx in 0..n {
            if target_idx == source_idx {
                continue;
            }
            let weight = self.connections[source_idx][target_idx];
            if weight < 0.05 {
                continue;
            }

            // Pondérer l'activation source par le poids de connexion
            let source_activation = activations[source_idx].as_ref().unwrap();
            let weighted: Vec<f32> = source_activation.iter().map(|x| x * weight).collect();

            let target_dim = self.organs[target_idx].organ.role.state_dim();
            let padded_target = pad_or_truncate(&weighted, target_dim);
            let target_output = self.organs[target_idx].process(&padded_target);
            activations[target_idx] = Some(target_output.output.clone());
            balance_loss_total += target_output.balance_loss;
        }

        self.tick += 1;

        NetworkPropagation {
            tick: self.tick,
            source: source_idx,
            activations,
            balance_loss: balance_loss_total,
        }
    }

    /// Renforcer la connexion entre deux organes (Hebbian)
    pub fn strengthen_connection(&mut self, a: usize, b: usize, delta: f32) {
        self.connections[a][b] = (self.connections[a][b] + delta).clamp(0.0, 1.0);
        self.connections[b][a] = (self.connections[b][a] + delta).clamp(0.0, 1.0);
    }

    /// Statistiques réseau
    pub fn stats(&self) -> NetworkStats {
        let total_experts: usize = self.organs.iter().map(|o| o.moe.config.n_experts).sum();
        let active_connections = self
            .connections
            .iter()
            .flat_map(|row| row.iter())
            .filter(|&&w| w > 0.05)
            .count();

        NetworkStats {
            n_organs: self.organs.len(),
            total_experts,
            active_connections,
            tick: self.tick,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkPropagation {
    pub tick: u64,
    pub source: usize,
    pub activations: Vec<Option<Vec<f32>>>,
    pub balance_loss: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub n_organs: usize,
    pub total_experts: usize,
    pub active_connections: usize,
    pub tick: u64,
}

/// Padder ou tronquer un vecteur à la taille cible
fn pad_or_truncate(v: &[f32], target: usize) -> Vec<f32> {
    if v.len() >= target {
        v[..target].to_vec()
    } else {
        let mut padded = v.to_vec();
        padded.resize(target, 0.0);
        padded
    }
}
