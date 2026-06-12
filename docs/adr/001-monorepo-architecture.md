# ADR-001 : Architecture Monorepo Unifiée

## Date
2026-05-26

## Contexte
SoulSystem a commencé comme plusieurs repos séparés (soullink-node, AVID,
scirust), causant fragmentation, conflits de versions, et difficulté de
déploiement.

## Décision
Migration vers un monorepo unique avec workspace Cargo. Tous les crates
partagent la même version de dépendances et le même pipeline CI.

## Conséquences
### Positives
- Une seule version de dépendances
- CI unifiée
- Réutilisation de code facilitée

### Négatives
- Taille du repo (~50 crates)
- Temps de compilation augmenté
- Risque de couplage accidentel entre crates

## Alternatives envisagées
- Multi-repo avec sous-modules git (abandonné : conflits fréquents)
- Packages npm/Python (abandonné : Rust plus performant pour le cœur)
