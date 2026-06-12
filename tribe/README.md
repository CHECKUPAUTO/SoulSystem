# TRIBE (Legacy) — Snowflake Arctic Embedding Server

**⚠️ REMPLACÉ PAR OLLAMA**

Ce dépôt archive le code du serveur d'embeddings TRIBE qui tournait sur
l'écosystème SoulLink/OpenClaw. Remplacé par `nomic-embed-text` via Ollama
(API compatible, plus léger, pas de dépendances GPU lourdes).

## Architecture Legacy

TRIBE était un serveur Python exposant `POST /embed` avec embeddings
Snowflake Arctic 768-dim, avec :
- Chunking avancé (tribe_advanced_chunking.py)
- Hybrid search (BM25 + embeddings)
- RAG pipeline
- Cache Nvidia TRT / NeMo
- Mass ingestion & production pipelines

## Remplacement

```bash
# Au lieu de:
curl http://127.0.0.1:7440/embed -d '{"texts":["hello"]}'

# Utiliser Ollama:
curl http://127.0.0.1:11434/api/embeddings \
  -d '{"model":"nomic-embed-text","prompt":"hello"}'
```

Le client TRIBE a été migré vers `synergie::embed` qui appelle directement
l'API Ollama. Nomic-embed-text (274 MB) remplace Snowflake Arctic (multi-GB).

## Contenu

- `src/` — Serveur Python + endpoints + ingestion
- `extensions/` — NeMo, TRT, RAG, CUDA, multimodal
- Fichiers MSA server (port 7430)

## Historique

TRIBE a été l'épine dorsale des embeddings de SoulLink pendant la phase V12.
La migration vers Ollama a eu lieu le 2026-05-14.
