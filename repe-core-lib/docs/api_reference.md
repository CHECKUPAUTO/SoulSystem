# Référence API

Ce document liste les fonctions et modules exposés par `repe_core_lib`.

## Module `repe_core_lib` (Python Extension)

### `check_ffi_bridge(ptr, rows, cols)`

Fonction de test pour vérifier la validité du pont FFI.

- **Arguments** :
    - `ptr` (int/pointer) : Adresse mémoire brute du tampon de données (float32).
    - `rows` (int) : Nombre de lignes de la matrice.
    - `cols` (int) : Nombre de colonnes de la matrice.
- **Retourne** : `bool` - `True` si l'opération d'écriture/lecture sur la mémoire a réussi, `False` sinon.
- **Description** : Cette fonction tente d'incrémenter puis de restaurer la valeur à l'index (0, 0) de la matrice pointée par `ptr`.

---

## API Interne (Rust)

### Module `ffi_api`

#### `wrap_raw_mut(ptr: *mut f32, rows: usize, cols: usize) -> MatrixViewMut`

Crée une vue mutable sur une matrice brute.
*Note : Utilise une supposition de layout "Row-major" (stride_row = cols, stride_col = 1).*

#### `wrap_raw(ptr: *const f32, rows: usize, cols: usize) -> MatrixView`

Crée une vue immuable sur une matrice brute.

---

## Modules en cours de développement

Les modules suivants sont déclarés dans `lib.rs` mais leurs implémentations sont à venir :

- `extractor` : Destiné aux opérations d'extraction de données.
- `surgeon` : Destiné aux opérations de modification structurelle de matrices.
