# `forge-bridge` — Pont Forge ↔ SoulSystem

Pont d'intégration entre le moteur évolutionnaire `forge-core` (moteur
strict, anti-triche visible, pareto multi-objectif) et l'écosystème
SoulSystem (synergie, openevolve-bridge, mesh-bridge, …).

## Rôle

`forge-core` est volontairement petit et autonome : il ne dépend de rien
d'autre que de la `std`, `rayon`, `rand`, `serde`. C'est ce qui lui
permet d'être audité, exécuté sur Thor, et branché en sandbox.

`forge-bridge` est la **vue fonctionnelle** consommable par les autres
briques :

| Type exposé         | Rôle                                                  |
|---------------------|-------------------------------------------------------|
| `ForgeConfig`       | Alias de `forge_core::Config` (sémantique «pont»)     |
| `ForgeCampaign<D>`  | Constructeur d'une campagne : `new(cfg, domain).run()`|
| `ScoreDto`          | Vue JSON-stable d'un `Score`                          |
| `CandidateDto`      | Vue JSON-stable d'un `Candidate` (id + repr)          |
| `binpack_demo`      | `Domain` de démo (bin-packing paramétrique)           |
| `llm_ollama`        | Stub — l'implémentation réelle vit dans `forge-core` feature `llm` |

## Anti-triche

Le pont ne fait que déléguer. Il ne calcule aucun score, ne consulte
aucune mesure, ne touche pas la porte `verify`. Si `forge-core` est
fiable, `forge-bridge` l'est aussi. Aucun raccourci possible.

## Tests

```bash
cargo test -p forge-bridge
```

Trois niveaux de couverture :

1. **Unit (16 tests, `src/lib.rs`)** — DTOs, conversions, déterminisme,
   signature non-panique, scénarios minimaux (survivors=1, pop=2).
2. **Intégration (8 tests, `tests/campaign.rs`)** — campagnes complètes
   binpack, holdout cohérent, DTOs JSON shape stable, divergences entre
   graines, round-trip `PackHeuristic` via JSON.
3. **Smoke (2 tests, `bridge-integration-tests/tests/forge_smoke.rs`)** —
   le 9e bridge de la suite smoke globale.

Total : **26 tests verts** (16 + 8 + 2).

## Limites connues

- Pas de transport HTTP dans ce crate. C'est le rôle du binaire
  `forge-service` (à venir, port 7890 dans la convention 7xxx des
  bridges). Ce crate reste pur Rust typé.
- Le stub `llm_ollama` n'ouvre pas de socket : pour brancher un vrai
  LLM, activer la feature `llm` de `forge-core` et implémenter
  `Domain::mutate` côté domaine "code" (compression, quantification,
  kernels, routage MoE).
