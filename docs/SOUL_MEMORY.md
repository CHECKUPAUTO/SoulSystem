# SoulMemory — Base de connaissances vectorielle

## Architecture

SoulMemory stocke les interactions, decouvertes AVID et optimisations
OpenEvolve sous forme de vecteurs pour enrichir les prompts LLM.

```
┌──────────────┐     ┌──────────────────┐
│  Clawd/AVID  │────▶│  SoulMemory      │
│  (store)     │     │  • vectorisation  │
└──────────────┘     │  • indexation     │
                     │  • recherche      │
┌──────────────┐     └────────┬─────────┘
│  Prompt Gen  │◀─────────────┘
│  (context)   │
└──────────────┘
```

## Moteurs d'embedding

### SciRustEmbedder (defaut, 64-dim)
Projection aleatoire deterministe (Johnson-Lindenstrauss) utilisant
8 fonctions de hachage par n-gramme. Preserve la similarite cosinus.
Leger, zero dependance externe, deterministe.

### NGramEmbedder (legacy, configurable dim)
N-grammes positionnels avec hachage simple. Conservé pour compatibilite.

### Interface pluggable
Le trait `Embedder` permet d'injecter n'importe quel moteur d'embedding:

```rust
let mem = SoulMemory::with_embedder(Box::new(MonEmbedder::new()))?;
```

## Oubli et priorisation

Chaque entree a un champ `importance: f32` (defaut 1.0). L'importance
initiale est calculee par `compute_initial_importance()` selon:
- Longueur du texte
- Presence de mots-cles (securite, bug, critique, etc.)
- Source (audit > user_feedback > research > autres)

### decay_and_prune

```rust
// Applique decay 0.99, seuil 0.1, max 10_000 entrees
let (kept, removed) = mem.decay_and_prune(0.99, 0.1, 10_000)?;
```

- Chaque entree: `importance *= decay_factor`
- Supprime si `importance < threshold`
- Si `count > max_entries`, supprime les moins importantes

Appeler periodiquement (toutes les 24h ou manuellement).

## Configuration

- **Qdrant** (recommande) : definit `QDRANT_URL=http://localhost:6334`
- **Fallback local** (automatique) : utilise sled

## Usage

```rust
let mem = SoulMemory::new()?;
let mut meta = HashMap::new();
meta.insert("source".into(), "arxiv".into());
mem.store("Quantum paper...", meta).await?;

let ctx = mem.get_context("quantum").await?;
// Injecter ctx dans le prompt LLM
```
