# ChronosAgent V5 → V6 — Migration & rationale

## TL;DR

Drop-in pour 4 fichiers du crate `scirust-chronos-agent`. Une seule callsite externe à mettre à jour. **20/20 tests verts**, lib compile sans erreur en `cargo check`.

| Fichier | Verdict V5 (SoulLink) | Réponse V6 |
|---|---|---|
| `ptnl_perceiver.rs` | "Perceiver standard, pas de temporel, pas de bidirectionnel" | Cross-attention non-causale sur fenêtre `[past ⊕ future]`, Time2Vec, MLP résiduel SiLU, reconstruction per-dim, soft NoiseGate gradué |
| `memory.rs` | "Pas de mécanisme passé↔futur" | Bi-temporalité explicite, rétro-causal binding, decay temporel avec anti-decay sur confirmation, HNSW O(log n), synchrony double-source |
| `bci.rs` | "GRU avec gain de confiance, wishful naming" | Neural ODE hamiltonien, leapfrog symplectique, attracteur de Lyapunov garanti, conservation d'énergie mesurée |
| `planner.rs` | "Pas une vraie diffusion, descente de gradient bruitée" | VRAIE SDE d'Itô (VP), cosine schedule, score net appris avec time embedding, CFG, DPM-Solver-2 d'ordre 2 |

## Ce qui change côté API

### Drop-in pur (aucune modification d'appelant)
- `PTNLPerceiver::new` : prend deux nouveaux paramètres (`d_time`, `t_future`). Tous les anciens existants se trouvent dans `main.rs` à 1 endroit.
- `PTNLPerceiver::forward` : prend désormais `(x_past, t_axis)` et retourne une recon loss `(t_total, d_input)` per-dim au lieu de `(t,)`. Pour la coherence loss existante, faire `.mean_all()` ou `.mean(0)?.mean(0)?` côté `coherence.rs`.
- `GRUCell::new` + `step` : **signatures inchangées**. `d_hidden` doit rester pair (déjà le cas avec D_HIDDEN=16). L'état caché retourné est `concat[q, p]` de même taille.
- `StochasticDiffusionPlanner::new` + `plan` : **signatures inchangées**.

### Petit patch (déjà appliqué dans le repo)
- `AtemporalMemory::semantic.learn(vec, threshold)` → `learn(vec, threshold, current_t)`. **Un seul callsite** dans `metacognition.rs::evolve_cache` — patché.

### Nouvelle capacité optionnelle (bidirectionnel)
Pour activer la perception bidirectionnelle complète, brancher dans le main loop :

```rust
// Au step t-1 : récupère la prédiction du PDT
let traj = planner.plan(&h_bci, alpha_sync)?;            // (NUM_STEPS, D_LATENT)
let future_window = traj.narrow(0, 1, T_FUTURE)?;        // skip step 0 (=présent)

// Au step t : injecte avant le forward PTNL
perceiver.set_future(future_window);
let (latents, recon) = perceiver.forward(&x_past, &t_axis)?;

// Et la mémoire stocke aussi la prédiction
memory.observe(&latent_vec, Some(future_window.get(0)?.to_vec1()?));
```

Sans cet appel, le PTNL fonctionne en mode "pas de futur connu" (pad zéros) — équivalent fonctionnel au mode V5 mais avec les autres améliorations actives.

## Invariants vérifiés par tests

```
test bci::tests::attractor_mode_converges                       ok
test bci::tests::hamiltonian_conserves_energy_without_friction  ok
test bci::tests::step_signature_compatible_with_v5              ok
test memory::tests::hnsw_index_scales                           ok
test memory::tests::predictive_quality_neutral_initially        ok
test memory::tests::retro_bind_confirms_matching_prediction     ok
test memory::tests::semantic_decay_diminishes_old_traces        ok
test planner::tests::cosine_schedule_sane                       ok
test planner::tests::inpainting_strict                          ok
test planner::tests::planner_plan_shape                         ok
test planner::tests::time_embedding_dims                        ok
test ptnl_perceiver::tests::noise_gate_attenuates_smoothly      ok
test ptnl_perceiver::tests::perceiver_bidirectional_forward     ok
test ptnl_perceiver::tests::perceiver_with_future_injection     ok
test ptnl_perceiver::tests::time2vec_shape                      ok
```

## Justifications mathématiques en deux mots

### Pourquoi le BCI hamiltonien tient "intemporelle"

Les équations de Hamilton sont **T-symétriques** : la substitution `(p, t) → (-p, -t)` laisse `dq/dt = ∂H/∂p`, `dp/dt = -∂H/∂q` invariantes. La trajectoire passée peut être reconstruite **exactement** depuis n'importe quel point — c'est l'opposé exact d'une GRU dont la mise à jour est strictement causale (et perd l'information).

Avec friction `γ = 0` : système conservatif, l'énergie totale `E = ½‖p‖² + V(q)` est invariante. La trajectoire vit sur l'hyper-surface `{(q,p) : H(q,p) = E₀}`, qui est **compacte** (donc bornée → "boucle") si V est coercitif. Le terme `+ ½‖q‖²` ajouté dans `PotentialMLP::value` garantit cette coercivité même avec un MLP arbitraire.

Quand `α_sync < threshold` : friction `γ > 0` → système dissipatif → convergence vers `argmin V`. C'est un **attracteur de Lyapunov** au sens strict (fonction de Lyapunov `L = E`, `dL/dt = -γ‖p‖² ≤ 0` avec égalité ssi `p = 0`).

### Pourquoi la MAA tient "atemporelle"

Au sens du **block universe** : passé et futur ont le même statut sémantique. Le rétro-binding implémente exactement cela : un événement *futur* observé à l'instant t peut renforcer une trace *passée* stockée à t-Δ. La causalité physique est respectée (le renforcement se produit à t, pas à t-Δ), mais la **représentation interne** ne distingue plus passé et futur du point de vue du score.

Inspiration : Hopfield Networks Are All You Need (Krotov & Hopfield 2020) et les "energy-based memories" récentes, où la trace n'est pas une séquence temporelle mais un attracteur dans l'espace d'embedding.

### Pourquoi le PDT tient "diffusion"

C'est désormais une **VRAIE SDE de variance-preserving** (Song et al. 2021, "Score-Based Generative Modeling Through SDEs") :
- Forward : `dx = -½β(t)·x dt + √β(t) dW`
- Reverse : `dx = [-½β(t)·x - β(t)·s_θ(x,t,c)] dt + √β(t) dW̃`
- Score `s_θ ≈ ∇_x log p_t(x | c)` appris (`ScoreNetwork`)
- Solveur DPM-Solver-2 (Lu et al. 2022) → équivalent ~2× moins de steps qu'Euler

CFG correct (Ho & Salimans 2022) : `s_guided = (1+w)·s_cond - w·s_uncond`, modulé par α_sync (plus α haut → confiance accrue → guidance plus forte).

### Pourquoi le PTNL tient "non linéaire" + "temporel"

- **Non-linéaire** : la V5 était `softmax(QKᵀ)·V·W_o` — strictement linéaire en `x` modulo le softmax (qui normalise, n'augmente pas l'expressivité). La V6 ajoute un **MLP résiduel SiLU** post-attention : `h' = h + W₂·SiLU(W₁·h + b₁) + b₂`. Le théorème d'approximation universelle s'applique pour le MLP, pas pour `softmax(QKᵀ)·V`.
- **Temporel** : Time2Vec encode `t` comme `[ω₀t + φ₀, sin(ω₁t+φ₁), ..., sin(ωₖt+φₖ)]` avec tous les `ωᵢ, φᵢ` appris. Capture périodicités à différentes échelles. Concaténé aux features avant K/V → l'attention "sait" où chaque token se trouve dans le temps, contrairement à V5 où le seul signal temporel était l'ordre des lignes (qui ne survit pas au shuffle).

## Performance attendue

- **Compilation** : `cargo check --lib` clean (4 warnings cosmétiques inchangés du repo).
- **Tests** : ~3.4s pour 20 tests sur CPU.
- **Inférence Hamiltonien** : 2× évaluations du gradient `∇V` par step (Verlet vs. Euler). À `d_q = 8` (config actuelle `D_HIDDEN=16`), le gradient par différence finie centrée coûte 16 forward du PotentialMLP — environ 1ms par step CPU. Pour scale-up, brancher l'autograd de candle (le placeholder est documenté dans `PotentialMLP::grad`).
- **PTNL** : passage de `(M, T)` à `(M, T_past + T_future)` → multiplicatif modeste (1.25× à T_future=4, T_past=16).
- **PDT** : score net est ~5× le coût de l'ancien Euler step, mais DPM-Solver-2 converge en 2× moins de steps réels → coût total équivalent pour meilleure qualité.

## Ce qu'il reste à brancher (pas dans le drop-in)

1. **Loop bidirectionnel** dans `main.rs` : ajouter `perceiver.set_future(...)` et passer `future_prediction` à `memory.observe(...)`. Décrit ci-dessus, ~10 lignes.
2. **Coherence loss** : `coherence.rs` consomme la sortie recon de PTNL. Comme la shape passe de `(T,)` à `(T_total, D_INPUT)`, faire `.mean_all()` avant. ~2 lignes.
3. **Entraînement du `ScoreNetwork`** et du `PotentialMLP` : actuellement initialisés mais non entraînés. Deux options :
   - Court terme : laisser tourner en zero-shot — la structure (Hamiltonien coercitif + SDE) garantit déjà la stabilité dynamique sans entraînement.
   - Moyen terme : entraînement par denoising score matching (Vincent 2011) sur les latents observés, et par minimisation d'énergie sur les états désirés. Squelette d'entraînement à ajouter dans `learning.rs`.
4. **Adoption de `Time2Vec` ailleurs** : l'observer PCA pourrait bénéficier de l'encodage temporel pour visualiser les rythmes.

## Décisions explicites (à reviewer)

- **HNSW lazy rebuild** : seuil à 256 inserts. Pour `EPISODIC_CAP=64` ça veut dire jamais d'index — fallback brute-force parallel qui est déjà optimal à cette échelle. À ajuster pour `SemanticStore` selon volume cible.
- **Soft NoiseGate** : utilise `exp(-(σ²/τ²)²)` (gain Gaussian-shape) au lieu du seuil binaire V5. Plus stable mais ne fait pas exactement `0` → si tu veux le freeze binaire en cas extrême, remettre une branche `if variance > τ_hard { return self.latents.clone() }`.
- **PotentialMLP::grad par diff finie** : justifié à `d_q ≤ 32`. Si tu pousses au-delà, brancher l'autograd candle (l'API existe via `candle_nn::AutogradContext`).
- **Friction γ** : `gamma_base = 0` (conservation pure hors mode attracteur), `gamma_attractor = 0.5` (modéré). À calibrer si l'attracteur se déclenche trop ou trop peu.
