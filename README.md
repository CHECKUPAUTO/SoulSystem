# repe_core_lib

`repe_core_lib` est une bibliothèque Rust haute performance conçue pour servir de pont FFI (Foreign Function Interface) entre Python et le moteur de calcul `scirust`. Elle permet des manipulations de matrices avec un mécanisme **Zero-Copy**, garantissant une efficacité maximale lors du transfert de données entre les deux langages.

## Fonctionnalités Clés

- **Mécanisme Zero-Copy** : Accès direct à la mémoire Python depuis Rust sans duplication de données.
- **Intégration PyO3** : Interface fluide avec Python via des modules d'extension natifs.
- **Vues de Matrices SciRust** : Utilisation des structures `MatrixView` et `MatrixViewMut` de `scirust-core`.
- **Optimisation Native** : Conçu pour tirer parti des instructions CPU spécifiques lors de la compilation.

## Structure du Projet

- `src/lib.rs` : Point d'entrée du module Python et définition des fonctions exposées.
- `src/ffi_api.rs` : Logique interne pour le wrapping des pointeurs bruts en vues de matrices.
- `docs/` : Documentation détaillée sur l'architecture et l'API.

## Installation

### Prérequis

- Rust (dernière version stable)
- Python 3.7+
- `maturin` : `pip install maturin`

### Compilation

Pour compiler et installer la bibliothèque dans votre environnement Python actuel :

```bash
maturin develop --release -C target-cpu=native
```

*Note : Assurez-vous que la dépendance `scirust-core` est correctement configurée dans le fichier `Cargo.toml`.*

## Utilisation Rapide (Python)

```python
import repe_core_lib
import numpy as np

# Exemple de vérification du pont FFI
data = np.array([[1.0, 2.0], [3.0, 4.0]], dtype=np.float32)
ptr = data.ctypes.data
rows, cols = data.shape

success = repe_core_lib.check_ffi_bridge(ptr, rows, cols)
print(f"FFI Bridge fonctionnel : {success}")
```

## Documentation

Pour plus de détails, consultez les fichiers dans le répertoire `docs/` :
- [Architecture](docs/architecture.md)
- [Référence API](docs/api_reference.md)

## Licence

[Spécifier la licence ici, ex: MIT]
