# Architecture

## Overview

SoulLink Orchestrateur v3 est une réimplémentation en Rust de l'orchestrateur Python original. Il utilise une architecture moderne et performante basée sur Tokio et Axum.

## Components

### 1. State Management

```rust
AppState {
    brains: DashMap<String, BrainConfig>,  // Lock-free concurrent hashmap
    query_count: AtomicU64,
    spawn_count: AtomicU64,
    brain_dir: String,
}
```

**Pourquoi DashMap ?**
- Accès concurrent sans verrous
- Performance supérieure à `RwLock<HashMap>`
- API compatible avec `std::collections::HashMap`

### 2. HTTP Client

Utilisation de `minreq` au lieu de `reqwest`:
- **Léger**: ~50KB vs ~2MB
- **Rapide**: Moins d'overhead pour les requêtes simples
- **Suffisant**: Pas besoin de features avancées pour notre use case

### 3. Routing

Le routing est **turbulence-aware**:
1. Sélectionne les 3 cerveaux les plus pertinents (match sur spécialités)
2. Récupère leur état de turbulence en parallèle
3. Ré-ordonne par stabilité (StableOrbit > DeepBasin > Transient > StrangeAttractor)
4. Effectue les appels dans cet ordre

### 4. Parallel Calls

```rust
let mut set = JoinSet::new();

for key in keys {
    set.spawn(async move {
        call_brain(&url, &endpoint, body).await
    });
}

while let Some(result) = set.join_next().await {
    // Process result
}
```

Utilise `tokio::task::JoinSet` pour:
- Lancer tous les appels simultanément
- Récupérer les résultats au fur et à mesure
- Annuler proprement si nécessaire

## Data Flow

```
Client Request
    ↓
Axum Router
    ↓
Route Handler
    ↓
Select Brains (score matching)
    ↓
Check Turbulence (parallel calls)
    ↓
Sort by Stability
    ↓
Query Brains (parallel calls)
    ↓
Merge Concepts
    ↓
JSON Response
```

## Performance Optimizations

1. **Zero-copy où possible**: Utilisation de références plutôt que clones
2. **Connection pooling**: Implicit via `minreq`
3. **Lock-free registry**: `DashMap` pour les cerveaux
4. **Compile-time optimizations**: 
   - `opt-level = 3`
   - `lto = "thin"`
   - `codegen-units = 1`
   - `panic = "abort"`

## Metrics

Endpoint `/metrics` expose:
- `soullink_queries_total`: Compteur de requêtes
- `soullink_spawns_total`: Compteur de spawns
- `soullink_brains_registered`: Gauge de cerveaux

Compatible avec Prometheus et Grafana.

## Security

Le service systemd inclut:
- `NoNewPrivileges=true`
- `PrivateTmp=true`
- `ProtectSystem=strict`
- `ProtectHome=true`
- Accès limité à `ReadWritePaths=/mnt/nvme/soullink_brain`
