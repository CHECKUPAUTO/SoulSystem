# Architecture de repe_core_lib

Ce document décrit les principes architecturaux de `repe_core_lib` et son rôle dans l'écosystème SciRust.

## Vue d'ensemble

`repe_core_lib` agit comme une couche de liaison haute performance entre l'environnement Python et les noyaux de calcul écrits en Rust. L'objectif principal est de permettre des calculs intensifs sur des données gérées par Python (typiquement des tableaux NumPy ou des tenseurs PyTorch) sans le coût de la sérialisation ou de la copie de mémoire.

## Mécanisme Zero-Copy

Le cœur de la bibliothèque repose sur le transfert de pointeurs bruts de Python vers Rust.

1. **Côté Python** : On récupère l'adresse mémoire du tampon de données (ex: `array.ctypes.data`).
2. **FFI Bridge** : Cette adresse est passée à une fonction Rust via PyO3.
3. **Côté Rust** : La fonction `wrap_raw_mut` (dans `ffi_api.rs`) utilise `MatrixViewMut::from_raw_parts` de `scirust-core` pour créer une vue sur cette mémoire.

```rust
pub unsafe fn wrap_raw_mut(ptr: *mut f32, rows: usize, cols: usize) -> MatrixViewMut<'static, f32> {
    MatrixViewMut::from_raw_parts(ptr, rows, cols, cols, 1)
}
```

Cette approche permet à Rust de lire et de modifier les données directement dans le tas (heap) de Python.

## Intégration avec scirust-core

`repe_core_lib` dépend fortement de `scirust-core`, qui fournit :
- Les abstractions de matrices (`MatrixView`, `MatrixViewMut`).
- Les noyaux de calcul (kernels) optimisés.

Le fichier `Cargo.toml` doit pointer vers l'emplacement correct de `scirust-core` pour que la compilation réussisse.

## Modules Prévus

- **Extractor** : Pour l'extraction sélective de caractéristiques depuis les matrices.
- **Surgeon** : Pour la manipulation directe et la modification de structures de données complexes.
