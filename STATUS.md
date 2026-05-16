# STATUS — État de l'Écosystème SoulSystem

*Généré le 2026-05-16*

## Modules actifs

| Module | Statut | Dépendances | Notes |
|--------|--------|-------------|-------|
| `soul_memory` | ✅ Actif | sled, seahash | Stockage vectoriel local. Pas de Qdrant nécessaire. |
| `telemetry` | ✅ Actif | tracing-subscriber | Init OTLP configurable via `OTEL_EXPORTER_OTLP_ENDPOINT`. |
| `code_signing` | ✅ Actif | sha2, uuid | Vérification de signature ed25519. Clés dans `~/.soulsystem/authorized_keys`. |
| `audit_log` | ✅ Actif | sled, sha2, chrono | Chaîne de hachage immuable. Stockage dans `/var/log/soulsystem/audit.sled`. |
| `bus` | ✅ Actif | tokio broadcast | Bus de messages interne (256 messages de buffer). |
| `compute_backend` | ✅ Actif | — | Trait ComputeBackend + CpuFallback. CUDA si feature `gpu`. |
| `config` | ✅ Actif | toml | `soulsystem.toml` + surcharge par variables d'env `SOULSYSTEM_*`. |

## Modules en veille (intégrés, désactivables)

| Module | Statut | Condition de réactivation |
|--------|--------|--------------------------|
| `federated_learning` | ⏸️ Veille | Quand une deuxième instance SoulSystem sera déployée. |
| `meta_learning` | ⏸️ Veille | Quand OpenEvolve sera intégré comme dépendance directe. |
| `dev_dashboard` | ⏸️ Veille | Flag `--dev` au lancement (feature `dev`). |
| `discovery` | ⏸️ Veille | Quand mDNS sera nécessaire (multi-instance LAN). |
| `soul_wallet` | ⏸️ Veille | Quand un nœud Lightning sera disponible. |
| `swarm` | ⏸️ Veille | Quand 3+ instances seront déployées. |
| `jit_hnn` | ⏸️ Veille | Feature `jit` (Cranelift) — dépendances lourdes. |
| `hardware_autoscaler` | ⏸️ Veille | Mode monitoring uniquement. |

## Modules en backlog (documentés, non intégrés)

| Module | Dépôt | Documentation | Priorité |
|--------|-------|---------------|----------|
| `skill_marketplace` | SoulSystem | `docs/SKILL_MARKETPLACE.md` | Faible |
| `skill_api` | SoulSystem | `docs/SKILL_MARKETPLACE.md` | Faible |
| SDK Python/TS | SoulSystem | `sdk/README.md` | Faible |
| Nix sandbox | SoulSystem | `docs/NIX_SANDBOX.md` | Faible |
| Anomaly detection | SYNERGIE | `README.md` | Moyenne |
| Quantization int8 | scirust | `README.md` | Haute (Jetson AGX) |
| Homomorphic | scirust | `README.md` | Faible |

## Résultats des tests (26/26 ✅)

| Suite | Tests | Résultat |
|-------|-------|----------|
| audit_log_test | 2 | ✅ |
| bus_test | 2 | ✅ |
| code_signing_test | 2 | ✅ |
| federated_test | 3 | ✅ |
| meta_learning_test | 1 | ✅ |
| soul_memory_test | 3 | ✅ |
| lib (unit) | 11 | ✅ |
| integration_hello | 0 | ⏸️ (placeholders) |

## Build

- `cargo build` : ✅ 0 erreurs
- `cargo test` : ✅ 26/26
- `cargo build --release` : ✅
