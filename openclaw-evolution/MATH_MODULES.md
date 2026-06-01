# MATH_MODULES.md — Modules Mathématiques pour SoulLink HNN

## Vue d'ensemble

10 modules Rust purs (aucune dépendance ML externe) implémentant les équations manquantes pour SoulLink HNN. **2 572 lignes de code, 19 tests unitaires, 0 erreur de compilation.**

---

## 🔴 Priorité 1 — Impact immédiat HNN

### #1 `math/hessian.rs` — Hessienne complète H_ij = ∂²U/∂q_i∂q_j

**Fonctions clés :**
- `compute_hessian(energy_fn, q, h)` → Hessienne par différences finies centrées d'ordre 2
- `compute_gradient(energy_fn, q, h)` → gradient numérique
- `classify_critical_point(hessian, tol)` → `LocalMinimum | LocalMaximum | SaddlePoint | Degenerate`
- `morse_index(hessian)` → nombre de directions instables
- `hessian.eigenvalues_power(iter)` → valeurs propres par itération de puissance + déflation
- `hessian.curvature_along(v)` → courbure directionnelle v^T H v
- `hessian.condition_number()` → |λ_max| / |λ_min|

**Intégration dans `soullink_hnn/src/lib.rs` :**
```rust
use openclaw::math::hessian::{compute_hessian, classify_critical_point, CriticalPointType};

// Remplacer hessian_diagonal() par la Hessienne complète
let hess = compute_hessian(|q| energy_surface.potential(q), &state.q, 1e-4);

// Foresight : prédire les bifurcations
match classify_critical_point(&hess, 1e-3) {
    CriticalPointType::SaddlePoint { positive, negative } => {
        // Bifurcation détectée — explorer les directions instables
        let eigvals = hess.eigenvalues_power(50);
    }
    CriticalPointType::LocalMinimum => { /* bassin stable */ }
    _ => {}
}
```

### #2 `math/optimizer.rs` — Adam / AdamW

**Fonctions clés :**
- `Adam::adam(dim)` → Adam standard
- `Adam::adamw(dim)` → AdamW avec weight decay découplé
- `adam.step(params, gradients)` → un pas d'optimisation, retourne le delta
- `adam.lr_schedule(step, warmup, total)` → warm-up linéaire + cosine decay
- Gradient clipping intégré

**Intégration — remplacer `apply_reward()` :**
```rust
use openclaw::math::optimizer::Adam;

// Au lieu de apply_reward() qui modifie α,μ,β manuellement :
let mut optimizer = Adam::adamw(energy_surface.params.len());
optimizer.config.lr = 1e-3;

// À chaque reward :
let gradients = compute_gradient(|p| -reward_signal(p), &energy_surface.params, 1e-4);
optimizer.step(&mut energy_surface.params, &gradients);
```

### #3 `math/compression.rs` — SVD tronquée (LoRA)

**Fonctions clés :**
- `truncated_svd(matrix, rank, power_iter)` → U_r, Σ_r, V_r^T par itération randomisée
- `svd.compress(q)` → projeter q de dim n vers dim r
- `svd.decompress(compressed)` → reconstruire l'approximation
- `lora_decompose(delta_weights, rank)` → décomposer en deux matrices low-rank A, B
- `svd.energy_ratio` → ratio d'énergie capturée

**Intégration — compression du vecteur d'état :**
```rust
use openclaw::math::compression::{truncated_svd, lora_decompose};

// Compresser q de 4096 → 64
let state_history: Vec<Vec<f32>> = collect_state_vectors();
let svd = truncated_svd(&state_history, 64, 3);
println!("Énergie capturée: {:.1}%", svd.energy_ratio * 100.0);

// Pour chaque nouveau q :
let compressed = svd.compress(&q);  // 4096 → 64
// Stocker compressed en mémoire long terme
let restored = svd.decompress(&compressed);  // 64 → ~4096
```

---

## 🟡 Priorité 2 — Évolution naturelle

### #4 `math/langevin.rs` — SDE dp = -∇U dt + σ dW

**Intégration dans `creativity.rs` :**
```rust
use openclaw::math::langevin::{LangevinIntegrator, LangevinConfig, initial_state};

let config = LangevinConfig::thermalized(0.01, 1.0, creativity_temperature);
let integrator = LangevinIntegrator::new(config);
let mut state = initial_state(q.clone());

// Explorer l'espace de créativité
let trajectory = integrator.simulate_trajectory(
    &mut state,
    |q| (energy_surface.potential(q), energy_surface.gradient(q)),
    100,
);

// Simulated annealing pour converger
integrator.anneal(0.95);
```

### #5 `math/attention.rs` — Softmax Attention

**Intégration dans `social.rs` :**
```rust
use openclaw::math::attention::{scaled_dot_product_attention, MultiHeadAttention};

// Alignement des vecteurs latents entre organes
let organ_embeddings: Vec<Vec<f32>> = organs.iter().map(|o| o.latent_vector()).collect();
let attn = scaled_dot_product_attention(&organ_embeddings, &organ_embeddings, &organ_embeddings);

// Multi-head pour capturer différentes relations
let mha = MultiHeadAttention::new(256, 8);  // d_model=256, 8 têtes
let output = mha.forward(&organ_embeddings);
// output.weights = matrice d'attention entre organes
```

### #6 `math/rl.rs` — PPO

**Intégration — entraîner la surface d'énergie :**
```rust
use openclaw::math::rl::{PPOConfig, RolloutBuffer, LinearPolicy, ppo_loss, Transition};

let config = PPOConfig::default();
let mut policy = LinearPolicy::new(state_dim, n_actions);
let mut buffer = RolloutBuffer::new();

// Collecter des transitions
buffer.push(Transition { state, action, reward, old_log_prob, value, done });

// Calculer GAE
buffer.compute_gae(config.gamma, config.gae_lambda);

// Optimiser
let loss = ppo_loss(&buffer, &policy, &config);
```

### #7 `math/contrastive.rs` — InfoNCE Loss

**Intégration — aligner les concepts entre organes :**
```rust
use openclaw::math::contrastive::{info_nce_loss, info_nce_gradient, Projector};

let projector = Projector::new(256, 128, 64);  // input→hidden→output

// Projeter les embeddings des organes
let anchor = projector.project(&organ_a.embedding);
let positive = projector.project(&organ_b.embedding);  // même concept
let negatives: Vec<Vec<f32>> = other_organs.iter()
    .map(|o| projector.project(&o.embedding)).collect();

let loss = info_nce_loss(&anchor, &positive, &negatives, 0.07);
let grad = info_nce_gradient(&anchor, &positive, &negatives, 0.07);
```

---

## 🟢 Priorité 3 — Architecture future

### #8 `math/moe.rs` — Mixture of Experts

```rust
use openclaw::math::moe::{MixtureOfExperts, MoEConfig};

let config = MoEConfig {
    input_dim: 256, output_dim: 256,
    n_experts: 8, top_k: 2,  // 8 bassins, 2 actifs
    balance_coeff: 0.01,
};
let mut moe = MixtureOfExperts::new(config);
let output = moe.forward(&state_vector);
// output.balance_loss → ajouter à la loss pour éviter le collapse
```

### #9 `math/quantize.rs` — Quantification 3-bit

```rust
use openclaw::math::quantize::{quantize, dequantize, compression_ratio, QuantConfig};

let config = QuantConfig { block_size: 32, symmetric: true };
let quantized = quantize(&state_vector_f32, &config);  // f32 → 3-bit
let ratio = compression_ratio(&quantized);  // ~10x
let restored = dequantize(&quantized);  // 3-bit → f32

// Auto-quantize avec erreur cible
let qv = auto_quantize(&data, 0.05);  // erreur < 5%
```

### #10 `math/graph.rs` — GNN Message Passing

```rust
use openclaw::math::graph::{ConceptGraph, GNNStack, graph_readout};

let mut graph = ConceptGraph::new();
let evo = graph.add_node("evolution", embedding_evo);
let opt = graph.add_node("optimization", embedding_opt);
graph.add_edge(evo, opt, 0.8);

let gnn = GNNStack::new(256, 3);  // 3 couches
gnn.forward(&mut graph);  // message passing

let global = graph_readout(&graph, "mean");  // embedding global du graphe
```

---

## Déploiement

### Installation

Les modules sont dans `src/math/`. Ils sont déjà intégrés dans le build.

```bash
# Le ZIP contient tout le projet mis à jour
unzip -o openclaw-v0.2-math.zip -d /opt/openclaw-evolution
cd /opt/openclaw-evolution

# Rebuild
docker compose build --no-cache
make restart
```

### Vérifier les tests

```bash
# Dans le container ou en local
cargo test math
```

Sortie attendue :
```
running 19 tests
test math::hessian::tests::test_quadratic_hessian ... ok
test math::hessian::tests::test_saddle_point ... ok
test math::optimizer::tests::test_adam_converges ... ok
test math::optimizer::tests::test_adamw_weight_decay ... ok
test math::compression::tests::test_svd_rank1 ... ok
test math::compression::tests::test_compress_decompress ... ok
test math::langevin::tests::test_overdamped_converges_to_minimum ... ok
test math::attention::tests::test_softmax_sums_to_one ... ok
test math::attention::tests::test_self_attention ... ok
test math::rl::tests::test_gae_computation ... ok
test math::rl::tests::test_policy_forward ... ok
test math::contrastive::tests::test_info_nce_positive_lower ... ok
test math::moe::tests::test_moe_forward ... ok
test math::moe::tests::test_top_k_sums_to_one ... ok
test math::quantize::tests::test_quantize_dequantize ... ok
test math::quantize::tests::test_compression_ratio ... ok
test math::quantize::tests::test_pack_unpack_roundtrip ... ok
test math::graph::tests::test_message_passing ... ok
test math::graph::tests::test_gnn_stack ... ok
test result: ok. 19 passed; 0 failed
```

### Utilisation dans le code existant

Les modules sont exposés via `crate::math::*` dans le projet OpenClaw, ou via `openclaw::math::*` si utilisé comme dépendance Cargo.

Aucun module ne dépend d'une bibliothèque ML externe — tout est implémenté en Rust pur avec seulement `rand` et `serde`.
