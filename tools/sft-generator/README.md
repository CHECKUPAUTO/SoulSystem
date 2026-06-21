# sft-generator

Générateur de jeux de données **SFT** (Supervised Fine-Tuning) à partir du
monorepo SoulSystem. L'outil parse l'intégralité du workspace Rust avec `syn`
et produit des paires *instruction → réponse* (une ligne JSON par exemple)
montrant **comment utiliser et interagir avec les modules** réels : fonctions,
structs, enums, traits, docs de crate, tests authentiques, et **scénarios
inter-crates**.

Deux formats de sortie au choix : chat `messages`, Alpaca
`instruction/input/output`, ou les deux (`--format`).

Tout est **ancré dans le code réel** : signatures, commentaires de doc, champs,
variantes et corps de tests sont extraits de l'AST, jamais inventés. Les
scénarios sont des **compositions** : les briques (crates, types, méthodes,
constructeurs) sont réelles, seul le récit qui les relie est synthétisé.

## Pourquoi un générateur (et pas 550 000 paires écrites à la main)

Émettre des centaines de milliers de paires dans une conversation représenterait
des centaines de millions de tokens. Un générateur reproductible est la bonne
réponse : il lit la vérité-terrain du dépôt et émet le JSONL à la demande.

## Chiffres mesurés sur ce dépôt

Mesure réelle (parsing de `2202` fichiers `.rs`, `2193` parsés, 9 échecs) :

| Élément extrait | Quantité |
|---|---|
| Crates | 314 |
| Fonctions / méthodes `pub` | 10 317 |
| Structs `pub` | 3 616 |
| Enums `pub` | 693 |
| Traits `pub` | 144 |
| Tests réels (`#[test]` / `#[tokio::test]`) | 9 963 |
| Docs de module / crate (`//!`) | 1 358 |
| **Total éléments** | **26 091** |

Capacité de génération (mesurée, après déduplication exacte) :

| Cible | Commande | Paires distinctes |
|---|---|---|
| **Premium (organique, ×1)** | `--augment 1` | **70 444** |
| **150 000 (recommandé)** | `--augment 3 --limit 150000` | **150 000** |
| **550 000** | `--augment 9 --limit 550000` | **550 000** |
| Plafond | `--augment 12` | ~840 000 |

Le premium organique inclut ~1 000 paires de **scénarios** (844 workflows
intra-crate, 172 intégrations inter-crates, 5 panoramas de sous-système).

- Le **premium organique** (≈ 70 K) correspond aux angles *réellement distincts*
  (réponses différentes), sans augmentation.
- Au-delà, l'**augmentation** diversifie la formulation de l'instruction tout en
  conservant la réponse ancrée (technique standard d'instruction-tuning). Les
  rondes sont équilibrées : chaque élément reçoit la formulation canonique avant
  qu'une seconde/troisième ne soit ajoutée.

## Build

```bash
cargo build --release --manifest-path tools/sft-generator/Cargo.toml
```

L'outil est un **workspace isolé** (table `[workspace]` vide dans son
`Cargo.toml`, et listé dans `exclude` à la racine) : le construire ne déclenche
pas le build des ~313 crates du monorepo.

## Utilisation

```bash
# Jeu premium (organique)
./tools/sft-generator/target/release/sft-generator --out premium.jsonl

# Cible 150 000 (recommandée) + échantillon lisible
./tools/sft-generator/target/release/sft-generator \
    --augment 3 --limit 150000 --out sft_150k.jsonl --sample sample.jsonl

# 550 000
./tools/sft-generator/target/release/sft-generator \
    --augment 9 --limit 550000 --out sft_550k.jsonl

# Les deux formats à la fois (écrit sft.jsonl + sft.alpaca.jsonl)
./tools/sft-generator/target/release/sft-generator --format both --out sft.jsonl

# Statistiques d'extraction uniquement (rapide, n'écrit rien)
./tools/sft-generator/target/release/sft-generator --stats-only
```

### Options

| Flag | Défaut | Rôle |
|---|---|---|
| `--root <chemin>` | `.` | Racine du workspace à parser |
| `--out <fichier>` | `sft_dataset.jsonl` | Fichier de sortie JSONL |
| `--format <fmt>` | `messages` | `messages`, `alpaca`, ou `both` |
| `--augment <K>` | `1` | Nombre de formulations par paire (rondes) |
| `--limit <N>` | `0` (illimité) | Plafond de paires écrites |
| `--sample <fichier>` | — | Écrit un petit échantillon varié et lisible |
| `--sample-size <N>` | `150` | Taille de l'échantillon |
| `--stats-only` | — | Affiche seulement les statistiques d'extraction |

En mode `both`, le fichier Alpaca est dérivé de `--out` en insérant `.alpaca`
(ex. `sft.jsonl` → `sft.alpaca.jsonl`). L'échantillon est toujours au format
`messages` pour rester lisible.

## Formats de sortie

Une ligne JSON par exemple. Format chat `messages` (défaut) :

```json
{
  "messages": [
    {"role": "system", "content": "Tu es un assistant expert du codebase SoulSystem…"},
    {"role": "user", "content": "Dans le crate `soul_llm`, comment utiliser `generate` … ?"},
    {"role": "assistant", "content": "`generate` — …\n\nSignature :\n```rust\n…\n```"}
  ],
  "meta": {"source": "fn.usage", "crate": "soul_llm"}
}
```

Format Alpaca (`--format alpaca` ou `both`) :

```json
{
  "instruction": "Dans le crate `soul_llm`, comment utiliser `generate` … ?",
  "input": "",
  "output": "`generate` — …",
  "system": "Tu es un assistant expert du codebase SoulSystem…",
  "meta": {"source": "fn.usage", "crate": "soul_llm"}
}
```

Le champ `meta.source` permet de filtrer / pondérer par type. Un échantillon
varié (~96 lignes, format `messages`, couvrant toutes les catégories) est fourni
dans [`sample.jsonl`](./sample.jsonl).

## Catégories générées (`meta.source`)

| Tag | Contenu |
|---|---|
| `fn.usage` | Signature + exemple d'appel idiomatique |
| `fn.explain` | Paramètres et valeur de retour détaillés |
| `fn.signature` | Signature exacte |
| `fn.errors` | Gestion du `Result` (`?` / `match`) |
| `fn.async` | Appel correct d'une fonction `async` (`.await`) |
| `fn.import` | Le `use` exact (crate en underscores) |
| `struct.usage` | Construction (littéral, `Default`) |
| `struct.explain` | Rôle + champs |
| `struct.fields` | Liste des champs publics typés |
| `struct.derives` | Traits dérivés et ce qu'ils permettent |
| `enum.usage` | Variantes + `match` |
| `enum.explain` | Rôle de l'enum |
| `enum.errors` | Gestion des enums d'erreur |
| `trait.explain` | Méthodes requises |
| `trait.impl` | Squelette `impl` avec les vraies signatures |
| `trait.bound` | Usage comme borne générique |
| `test.usage` | Test réel du crate (code authentique, sliced) |
| `crate.overview` | À quoi sert le crate / module (`//!`) |
| `crate.example` | Exemple tiré de la doc du crate |
| `crate.exports` | Ré-exports publics principaux |
| `nav.location` | Où est défini un symbole (crate, module, fichier) |
| `scenario.workflow` | Workflow multi-étapes : `new` puis enchaînement de méthodes réelles |
| `scenario.pair` | Intégration de deux crates d'un même sous-système |
| `scenario.subsystem` | Panorama d'un sous-système et de ses types clés |

## Notes

- Les noms de crate à tirets (`soul-memory`) sont correctement convertis en
  identifiants Rust (`soul_memory`) dans tout code généré.
- Les corps de tests sont découpés ligne à ligne depuis la source (via les
  *span locations* de `proc-macro2`), donc **réels et compilables**.
- La déduplication exacte (hash de l'instruction) garantit l'absence de doublons
  stricts.
- Les gros fichiers `*.jsonl` sont volontairement ignorés par git
  (cf. `.gitignore`) ; seul `sample.jsonl` est suivi. Régénérez les jeux complets
  avec l'outil.
