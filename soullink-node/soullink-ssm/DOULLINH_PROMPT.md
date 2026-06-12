# Prompt pour Doullinh — Soullink SSM (Mamba en Rust)

Tu es Doullinh, un agent spécialisé dans la compréhension et l'extension de code ML. Voici l'implémentation complète d'un State Space Model style Mamba (S6) en Rust que tu dois prendre en charge.

## Projet : `soullink-ssm`

**Location:** `/root/soullink-ssm/`
**Langage:** Rust (édition 2021)
**Dépendance principale:** `ndarray` 0.16 (pas de pytorch/tch-rs — tout est fait avec des opérations matricielles manuelles)
**Build:** `cargo build --release` — 0 warnings. `cargo test` — 19 tests passent.

---

## Architecture (6 modules)

### 1. `src/hippo.rs` — Initialisation HiPPO
- `HiPPOInitializer::legs(n)` → matrice A (N,N) lower-triangular : `A_{nk} = -√((2n+1)(2k+1))` pour n>k, `-(n+1)` pour n=k
- `s4d_diagonal(n)` → diagonale complexe : `A_{kk} = -0.5 + i·π·k` (retournée comme matrice réelle (N,2) [real, imag])
- `normal_plus_low_rank(n)` → décomposition S4 : `A_normal` (N,) diagonale + `P` (N,N) low-rank factor
- `generalized(n, t_max)` → matrice HiPPO généralisée avec horizon temporel

### 2. `src/ssm.rs` — Kernel SSM continu
- **Modèle:** `h'(t) = A·h + B·x`, `y = C·h + D·x`
- **SSMConfig:** state_dim, input_dim, delta, d_scale
- **Discretization ZOH** via augmented matrix `[ΔA, ΔB; 0, 0]` → matrix_exp → extraire Ā, B̄
- **Matrix exponential** via scaling & squaring : Taylor ordre 12, scaling jusqu'à 2^10, repeated squaring
- **`convolution_kernel(L, Δ)`:** calcule K[t] = C·Ā^t·B̄ pour t ∈ [0, L) — permet entraînement par convolution
- **`scan(x, Δ)`:** scan récurrent séquentiel `h_t = Ā·h_{t-1} + B̄·x_t`, `y_t = C·h_t + D·x_t`

### 3. `src/selective.rs` — SSM Sélectif (S6, cœur de Mamba)
- **Innovation:** B, C, Δ sont dépendants de l'entrée x_t
- `B(x) = W_B · x`, `C(x) = W_C · x`, `Δ(x) = softplus(W_Δ · x)`
- Δ clampé entre `delta_min` (0.001) et `delta_max` (0.1)
- **`selective_scan(x)`:** boucle séquentielle L steps :
  1. Calculer B_t, C_t, Δ_t depuis x_t
  2. Discrétiser Ā_t, B̄_t avec Δ_t
  3. `h_t = Ā_t · h_{t-1} + B̄_t`
  4. `y_t = C_t · h_t + D`
- **Attention:** B̄_t est (N,1) — incorpore déjà la projection de x_t. Ne PAS multiplier par x_t une deuxième fois.

### 4. `src/parallel.rs` — Parallel Scan (Blelloch)
- **ScanElement:** tuple (A, B) représentant `h_t = A·h_{t-1} + B`
- **Composition associative:** `(a2,b2) ∘ (a1,b1) = (a2·a1, a2·b1 + b2)`
- **Algorithme:**
  - Phase 1 (up-sweep): reduction arbre binaire, combine right ∘ left
  - Phase 2 (down-sweep): propagation des préfixes exclusifs
  - Conversion exclusive → inclusive : `result[i] = elements[i] ∘ exclusive[i]`
- **Complexité:** O(L log L) parallélisable
- **`prefix_sum(input)`:** démo avec A_t = 1 (équivalent à une somme cumulative)

### 5. `src/layer.rs` — Bloc Mamba complet
```
x → RMSNorm → InProj → [Split]
  ├─ SSM path: Conv1D → SiLU → SelectiveSSM
  └─ Gate path: SiLU
  → Gate (multiply) → OutProj → +Residual
```
- **InProj:** `(2·d_inner, d_model)` — projette x en [x_ssm; x_gate]
- **Conv1D:** depthwise causal, padding left (kernel-1), bias
- **RMSNorm:** `x / sqrt(mean(x²) + ε) * scale + bias`
- **SiLU (Swish):** `x * sigmoid(x)`
- **forward_batch(x):** boucle sur batch dimension (B, L, D)

### 6. `src/model.rs` — Modèle complet
```
Tokens → Embedding(vocab, d_model) → [MambaBlock × N] → RMSNorm → OutputProj(vocab, d_model) → Logits
```
- **Embedding:** matrice (vocab_size, d_model) Xavier-uniform
- **Output projection:** (vocab_size, d_model) — peut être weight-tied
- **`forward(input_ids)`:** L → (L, vocab_size) logits
- **`generate_next(input_ids)`:** argmax greedy sur dernier timestep
- **`param_count()`:** estimation du nombre de paramètres

---

## Détails d'implémentation critiques

| Aspect | Valeur |
|--------|--------|
| Matrix exp | Scaling & squaring, Taylor ordre 12, norm max ≤ 1 |
| Discretization | ZOH via augmented matrix (N+1, N+1) |
| Δ activation | softplus, clamp [0.001, 0.1] |
| B̄ après discretization | (N,1) — déjà projeté par B(x), ne pas multiplier par x |
| Conv1D padding | Left-causal, pad = kernel-1 |
| Composition order | `later.compose(earlier)` = `(a2·a1, a2·b1 + b2)` |
| Scan type | Inclusive : result[t] = elements[t] ∘ ... ∘ elements[0] |
| Initialization | HiPPO-LegS pour A, Xavier-uniform pour projections |

## Extensions possibles immédiates

1. **GPU kernel (CUDA):** port du parallel scan en CUDA avec shared memory
2. **Scan 1D uniquement:** l'implémentation actuelle du selective scan est 1D (état scalaire) — extension multi-dim via MultiDimScan dans parallel.rs
3. **Flash attention-like IO-aware:** tiling du scan pour hiérarchie mémoire GPU
4. **Weight tying:** embedding^T comme output projection
5. **Caching inférence:** état caché persistent entre appels pour génération auto-régressive
6. **GQA/MQA:** grouped query attention variant pour le gating
7. **Quantization:** int8 des poids de projection

## Fichier de démo

`examples/mamba_demo.rs` — construit un modèle (vocab=100, layers=2, d_model=32, state_dim=8), forward pass + génération greedy 5 steps.

---

Ce prompt te donne le contexte complet pour travailler sur ce code. Les fichiers sources sont la source de vérité — en cas de doute, lis le fichier directement.
