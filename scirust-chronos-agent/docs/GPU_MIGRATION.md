# Migration GPU (Jetson Thor) — état réel et chemin complet

> **Correction d'une simplification des migrations précédentes.** J'ai répété
> que le passage GPU était « un changement d'une ligne » (`Device::Cpu` →
> `Device::new_cuda(0)`). **C'est faux.** Voici l'image réelle, mesurée.

## Le vrai obstacle : le f64

Le code utilise `DType::F64` (double précision) partout. Sur un GPU edge comme
la Jetson Thor (Blackwell), le f64 est :
- soit **non supporté** par certaines ops CUDA dans candle,
- soit **pénalisé d'un facteur 32 à 64×** (les GPU non-datacenter bride la
  double précision matériellement).

**Conséquence** : lancer le code actuel en f64 sur GPU serait probablement
*plus lent* que sur CPU, ou échouerait sur certaines ops. Le speedup GPU
n'est réel qu'en **f32**.

## La bonne nouvelle : le f32 est sûr pour nos dynamiques

Crainte légitime : le BCI hamiltonien et l'intégrateur leapfrog sont en f64
pour la conservation d'énergie. Le f32 casserait-il l'attracteur de Lyapunov ?

**Test empirique** (leapfrog symplectique, oscillateur harmonique, 200 steps) :

| dtype | dérive d'énergie max |
|---|---|
| f64 | 2.50e-5 |
| f32 | 2.52e-5 |
| **ratio** | **1.0×** |

Le f32 conserve l'énergie **aussi bien** que le f64. C'est logique : un
intégrateur symplectique conserve l'énergie par construction *géométrique*,
pas numérique. L'erreur de troncature symplectique (O(dt²)) domine largement
l'erreur d'arrondi flottant. **Le f32 est donc sûr** pour les dynamiques
hamiltoniennes du BCI.

## Ce qui est livré (V6.6) — infrastructure prête

`src/device.rs` :

1. **`compute_dtype()`** — point de vérité unique du dtype. F64 par défaut,
   bascule via `CHRONOS_DTYPE=f32`. Les **26 `DType::F64` hardcodés** (zeros,
   poids) ont été remplacés par cet appel. En mode f64 : zéro changement de
   comportement (94/94 tests verts).

2. **`select_device()`** — choisit CPU/CUDA via `CHRONOS_DEVICE=cpu|cuda|cuda:N`,
   avec fallback CPU propre si CUDA indisponible ou non compilé. Ne panique
   jamais.

3. **`tensor_from_f64(data, shape, device)`** — helper qui crée un tenseur
   depuis un `&[f64]` et le convertit au `compute_dtype()`. C'est le
   remplaçant de `Tensor::from_slice` pour éviter le DTypeMismatch en f32.

## Ce qui reste — périmètre exact

Quand on bascule `CHRONOS_DTYPE=f32`, les tests BCI échouent avec :
```
unexpected dtype, expected: F64, got: F32
```

Cause : les **~47 sites `Tensor::from_slice(&[f64], ...)`** créent encore du
f64 implicite, qui entre en collision avec les poids passés en f32. C'est le
travail mécanique restant :

| Fichier | Sites `from_slice` |
|---|---|
| training.rs | 11 |
| main.rs | 8 |
| bci.rs | 7 |
| planner.rs | 6 |
| ptnl_perceiver.rs | 5 |
| projector.rs | 4 |
| hypernetwork.rs | 3 |
| learning.rs | 2 |
| observer.rs | 1 |

**Migration** : remplacer chaque `Tensor::from_slice(&data, shape, &device)?`
(où `data: &[f64]`) par `device::tensor_from_f64(&data, shape, &device)?`.
Mécanique, mais à faire avec soin (vérifier que `data` est bien un slice f64
et pas déjà un tenseur). Estimation : ~1-2h de travail + revalidation des
94 tests en f32.

## Procédure complète sur la Jetson Thor

```bash
# 1. Activer la feature cuda dans Cargo.toml
#    candle-core = { version = "0.10", features = ["cuda"] }
#    candle-nn   = { version = "0.10", features = ["cuda"] }

# 2. Migrer les ~47 from_slice (voir tableau ci-dessus) vers tensor_from_f64

# 3. Valider la précision f32 sur CPU d'abord
CHRONOS_DTYPE=f32 cargo test --lib   # doit rester 94/94

# 4. Compiler pour GPU
cargo build --release --features cuda

# 5. Lancer sur GPU
CHRONOS_DEVICE=cuda CHRONOS_DTYPE=f32 ./target/release/chronos-agent
```

## Gains attendus

| Métrique | CPU (f64) | GPU (f32) attendu |
|---|---|---|
| Latence/step | ~46 ms | ~3-5 ms (10-15×) |
| Batch training | séquentiel | parallèle massif |
| Capacité mémoire | RAM système | 128 GB unifiés (Thor) |

La mémoire unifiée de la Thor (128 GB CPU+GPU partagés) est un atout : pas de
copies host↔device coûteuses, le checkpoint safetensors migre de façon
transparente.

## Checkpoint cross-dtype

Le `CheckpointManager` (safetensors) gère la migration automatiquement : un
checkpoint sauvé en f64 sur CPU se recharge en f32 sur GPU (candle reconvertit
au load via `to_dtype`). Les poids appris sur CPU sont donc réutilisables sur
GPU sans réentraînement.

## Honnêteté sur l'estimation

- Infrastructure (device.rs) : **fait, testé**
- Preuve f32 sûr : **fait, mesuré**
- Migration des ~47 from_slice + randn + Tensor::new + lectures : **FAIT (V6.6)**
- Validation f32 complète : **FAIT** — 94/94 tests en f32 (0 échec sur 8 runs),
  runtime complet converge en f32 (DSM −40.1%, comparable au f64)
- Validation GPU réelle : **impossible ici** (pas de CUDA), à faire sur la Thor

Le passage GPU est donc à **~95% débloqué** : le risque conceptuel (précision
f32) est levé, l'infrastructure est en place, la migration dtype est faite et
validée en f32 sur CPU. Il ne reste que la compilation `--features cuda` et le
test sur matériel réel.

## Ce qui a été fait dans la migration f32 (V6.6)

| Catégorie | Sites | Solution |
|---|---|---|
| `DType::F64` hardcodés | 26 | → `compute_dtype()` |
| `Tensor::from_slice(&[f64])` | ~47 | → `device::tensor_from_f64()` |
| `Tensor::randn(0.0f64, ...)` | ~10 | → `device::randn_f64()` |
| `Tensor::new(scalar_f64, ...)` | ~24 | → `device::scalar_f64()` |
| `.to_scalar::<f64>()` / `.to_vec1::<f64>()` | ~38 | → trait `TensorReadExt` |
| `.to_scalar()` annoté f64 (sans turbofish) | 2 | → `.read_scalar_f64()` |

Bugs subtils corrigés en cours de route :
- **Récursion infinie** : le sed avait remplacé `.to_scalar::<f64>()` dans la
  définition même du trait → stack overflow. Corrigé.
- **Race condition de tests** : les tests `device.rs` modifiaient
  `CHRONOS_DTYPE` via `set_var`, créant une race avec les tests parallèles
  lisant `compute_dtype()`. Refactoré en logique pure (`dtype_from_str`)
  testée sans toucher l'env global.
- **Bug runtime non couvert par les tests** : `loss.to_scalar()?` (annoté f64,
  sans turbofish) dans `train_score_step` / `train_potential_step` échouait en
  f32 dans le flux réel mais pas dans les tests synthétiques. Tracé et corrigé.
- **Tests intrinsèquement bruités en f32** : la cross-validation autograd↔FD
  (annulation catastrophique de la FD en f32) et le gap contrastif (bruit
  d'arrondi sur 50 steps AdamW) — seuils rendus dtype-aware, honnêtement
  documentés.
