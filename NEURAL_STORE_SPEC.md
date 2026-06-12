---
title: Neural Store (Digital Hippocampus) - Specs Techniques
type: technical-specification
source: user-provided-prompt
---

# 📑 Prompt de Spécifications Techniques : Neural Store (Digital Hippocampus)

## Vision Globale
Le Neural Store n'est pas une base de données vectorielle classique, mais un "hippocampe numérique" conçu pour un organisme IA autonome. Il privilégie le débit brut (throughput) et la latence minimale en saturant les capacités du CPU (SIMD) et du disque (NVMe), tout en implémentant des mécanismes biologiques d'auto-organisation et d'oubli.

## Architecture Technique Détaillée

### 1. Moteur de Calcul SIMD & Métriques (src/core/)
- Hiérarchie d'exécution : AVX-512 → AVX2 + FMA → ARM Neon → Scalar Fallback
- Primitives : Produit scalaire vectorisé et soustraction de vecteurs
- Cosinus : Fast Path pour vecteurs pré-normalisés (produit scalaire SIMD pur)
- Mahalanobis : Distance avec matrices d'inverse de covariance denses et diagonales
- Rayon pour distribuer les recherches sur 64+ cœurs

### 2. Couche de Persistance & Atomicité (src/storage/)
- Architecture inspirée des LSM-Trees
- Write-Ahead Log via memmap2
- Format atomique : [Longueur][Checksum CRC32][Payload]
- MemTable Lock-Free avec SkipMap concurrente (crossbeam-skiplist)

### 3. Workers d'Arrière-Plan (src/brain/)
- Clustering Dynamique par centroïdes
- Garbage Collection basée sur Score = Accès / (Âge + 1)
- BrainManager orchestre sans interférer avec le chemin critique

### 4. Interface Zero-Copy FFI (src/ffi/)
- Zéro copie via pointeurs bruts et std::slice::from_raw_parts
- Singleton global (OnceLock) et libération explicite (ns_free)

## Pipeline Opérationnel
1. Initialisation : Open → WAL → MemTable → Workers
2. Insertion : ns_put → sérialisation → WAL → Indexation Lock-Free
3. Requête : ns_search → normalisation → dispatch SIMD → scan parallèle → Top-K Sort
