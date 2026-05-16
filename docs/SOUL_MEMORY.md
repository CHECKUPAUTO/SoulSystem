# SoulMemory — Base de connaissances vectorielle

## Architecture

SoulMemory stocke les interactions, découvertes AVID et optimisations
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

## Configuration

- **Qdrant** (recommandé) : définit `QDRANT_URL=http://localhost:6334`
- **Fallback local** (automatique) : utilise sled + HNSW

## Usage

```rust
let mem = SoulMemory::new()?;
let mut meta = HashMap::new();
meta.insert("source".into(), "arxiv".into());
mem.store("Quantum paper...", meta).await?;

let ctx = mem.get_context("quantum").await?;
// Injecter ctx dans le prompt LLM
```
