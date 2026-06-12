#!/usr/bin/env python3
"""
tribe_msa_server_minimal.py
============================
Version minimale sans dépendances lourdes.
Fallback : embeddings aléatoires déterministes (hash-based).

Usage:
  python tribe_msa_server_minimal.py --port 7431
"""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import logging
import math
import os
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional

import numpy as np
import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

# Logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] TRIBE-MSA — %(message)s",
)
log = logging.getLogger("tribe-msa")

# Config
N_CPU_THREADS = int(os.getenv("CPU_THREADS", "64"))
N_VERTICES = 20_004  # fsaverage5

torch = None  # Not required for fallback
sentence_transformers = None  # Not required for fallback

# ══════════════════════════════════════════════════════════════════════════════
# §1  TRIBE RUNNER — FALLBACK ONLY
# ══════════════════════════════════════════════════════════════════════════════

class TribeRunner:
    """Fallback : embeddings aléatoires déterministes basés sur hash."""
    
    N_VERTICES = 20_004
    REGIONS = {
        "visual":      (0,     5_000),
        "auditory":    (5_000, 10_000),
        "prefrontal":  (10_000, 15_000),
        "motor":       (15_000, 20_004),
    }
    
    def __init__(self):
        self._executor = ThreadPoolExecutor(max_workers=N_CPU_THREADS)
        log.info(f"TRIBE fallback — {N_VERTICES} vertices, {N_CPU_THREADS} threads")
    
    def embed_text(self, text: str) -> np.ndarray:
        """Génère un embedding déterministe basé sur le hash du texte."""
        # Hash déterministe
        h = int(hashlib.sha256(text.encode()).hexdigest(), 16)
        rng = np.random.default_rng(h % (2**32))
        
        # Vecteur aléatoire déterministe
        vec = rng.standard_normal(self.N_VERTICES).astype(np.float32)
        vec = vec / (np.linalg.norm(vec) + 1e-8)
        return vec
    
    def embed_batch(self, texts: List[str]) -> List[np.ndarray]:
        """Batch parallèle."""
        futures = [self._executor.submit(self.embed_text, t) for t in texts]
        return [f.result() for f in futures]
    
    def cognitive_load(self, pattern: np.ndarray) -> float:
        """Activation préfrontale normalisée [0, 1]."""
        s, e = self.REGIONS["prefrontal"]
        pf = float(pattern[s:e].mean())
        mn, mx = float(pattern.min()), float(pattern.max())
        if mx == mn:
            return 0.5
        return round(float(np.clip((pf - mn) / (mx - mn + 1e-8), 0, 1)), 4)
    
    def get_dominant_regions(self, pattern: np.ndarray) -> List[str]:
        mean = pattern.mean()
        return [name for name, (s, e) in self.REGIONS.items()
                if pattern[s:e].mean() > mean]
    
    def cosine(self, a: np.ndarray, b: np.ndarray) -> float:
        na, nb = np.linalg.norm(a), np.linalg.norm(b)
        if na == 0 or nb == 0:
            return 0.0
        return round(float(np.dot(a, b) / (na * nb)), 4)


# ══════════════════════════════════════════════════════════════════════════════
# §2  BRAIN STORE
# ══════════════════════════════════════════════════════════════════════════════

@dataclass
class BrainEntry:
    chunk_id:         int
    doc_id:           int
    text_preview:     str
    brain_pattern:    np.ndarray
    cognitive_load:   float
    dominant_regions: List[str]
    timestamp:        float = field(default_factory=time.time)
    metadata:         Dict  = field(default_factory=dict)

class BrainStore:
    def __init__(self):
        self._entries: Dict[int, BrainEntry] = {}
        self._lock    = threading.Lock()
    
    def add(self, entry: BrainEntry):
        with self._lock:
            self._entries[entry.chunk_id] = entry
    
    def get(self, chunk_id: int) -> Optional[BrainEntry]:
        return self._entries.get(chunk_id)
    
    def all_ids(self) -> List[int]:
        return list(self._entries.keys())
    
    def search(self, query: np.ndarray, top_k: int = 10,
                brain_weight: float = 0.3,
                max_cognitive_load: float = 1.0) -> List[Dict]:
        results = []
        with self._lock:
            entries = list(self._entries.values())
        
        for e in entries:
            if e.cognitive_load > max_cognitive_load:
                continue
            
            # Similarité cosinus
            brain_sim = float(np.dot(query, e.brain_pattern) /
                             (np.linalg.norm(query) * np.linalg.norm(e.brain_pattern) + 1e-8))
            
            score = brain_sim  # Pas de composante sémantique dans cette version
            
            results.append({
                "chunk_id":       e.chunk_id,
                "doc_id":         e.doc_id,
                "text_preview":   e.text_preview[:200],
                "brain_score":    round(brain_sim, 4),
                "combined_score": round(score, 4),
                "cognitive_load": e.cognitive_load,
                "dominant_regions": e.dominant_regions,
            })
        
        results.sort(key=lambda x: x["combined_score"], reverse=True)
        return results[:top_k]
    
    def stats(self) -> Dict:
        with self._lock:
            n = len(self._entries)
            if n == 0:
                return {"n_entries": 0}
            loads = [e.cognitive_load for e in self._entries.values()]
            return {
                "n_entries":         n,
                "avg_cognitive_load": round(sum(loads)/n, 3),
                "ram_mb":            round(
                    sum(e.brain_pattern.nbytes for e in self._entries.values()) / 1e6, 1),
            }


# ══════════════════════════════════════════════════════════════════════════════
# §3  FASTAPI
# ══════════════════════════════════════════════════════════════════════════════

app = FastAPI(title="TRIBE + MSA Brain API", version="1.0-minimal")

# État global
tribe: Optional[TribeRunner] = None
store: Optional[BrainStore] = None
_ctr = 0
_lock = threading.Lock()

# Modèles
class EmbedRequest(BaseModel):
    text: str
    doc_id: int = 0
    metadata: Dict = {}

class SearchRequest(BaseModel):
    query: str
    top_k: int = 10
    brain_weight: float = 0.3
    max_cognitive_load: float = 1.0

class HallucinationRequest(BaseModel):
    query: str
    response: str
    threshold: float = 0.35

class EvaluateRequest(BaseModel):
    query: str
    candidates: List[str]
    reference: Optional[str] = None

class EmbedResponse(BaseModel):
    chunk_id: int
    cognitive_load: float
    dominant_regions: List[str]
    brain_dims: int
    latency_ms: float
    mode: str = "fallback"

# Routes
@app.get("/health")
async def health():
    import psutil
    ram = psutil.virtual_memory()
    return {
        "status": "ok",
        "mode": "fallback",
        "brain_entries": len(store.all_ids()) if store else 0,
        "ram_used_gb": round(ram.used / 1e9, 1),
        "ram_total_gb": round(ram.total / 1e9, 1),
        "cpu_threads": N_CPU_THREADS,
    }

@app.get("/info")
async def info():
    return {
        **await health(),
        "store_stats": store.stats() if store else {},
        "proposals": {
            "A_brain_memory": "/embed /search",
            "B_neural_grounding": "/evaluate /hallucination",
        },
    }

@app.post("/embed", response_model=EmbedResponse)
async def embed(req: EmbedRequest):
    global _ctr
    t0 = time.perf_counter()
    
    # Embed
    loop = asyncio.get_event_loop()
    brain = await loop.run_in_executor(None, tribe.embed_text, req.text)
    
    # Métadonnées
    cload = tribe.cognitive_load(brain)
    regions = tribe.get_dominant_regions(brain)
    
    with _lock:
        _ctr += 1
        cid = _ctr
    
    # Stocker
    entry = BrainEntry(
        chunk_id=cid,
        doc_id=req.doc_id,
        text_preview=req.text[:200],
        brain_pattern=brain,
        cognitive_load=cload,
        dominant_regions=regions,
        metadata=req.metadata,
    )
    store.add(entry)
    
    return EmbedResponse(
        chunk_id=cid,
        cognitive_load=cload,
        dominant_regions=regions,
        brain_dims=len(brain),
        latency_ms=round((time.perf_counter() - t0) * 1000, 1),
    )

@app.post("/search")
async def search(req: SearchRequest):
    t0 = time.perf_counter()
    
    loop = asyncio.get_event_loop()
    query_brain = await loop.run_in_executor(None, tribe.embed_text, req.query)
    
    results = store.search(
        query_brain,
        top_k=req.top_k,
        brain_weight=req.brain_weight,
        max_cognitive_load=req.max_cognitive_load,
    )
    
    return {
        "query": req.query,
        "n_results": len(results),
        "brain_weight": req.brain_weight,
        "results": results,
        "latency_ms": round((time.perf_counter() - t0) * 1000, 1),
    }

@app.post("/evaluate")
async def evaluate(req: EvaluateRequest):
    t0 = time.perf_counter()
    loop = asyncio.get_event_loop()
    
    all_texts = [req.query] + req.candidates
    embeddings = await loop.run_in_executor(None, tribe.embed_batch, all_texts)
    
    query_brain = embeddings[0]
    cand_brains = embeddings[1:]
    
    results = []
    for i, (cand, cb) in enumerate(zip(req.candidates, cand_brains)):
        q_sim = tribe.cosine(query_brain, cb)
        results.append({
            "index": i,
            "candidate": cand[:200],
            "alignment": q_sim,
            "cognitive_load": tribe.cognitive_load(cb),
        })
    
    results.sort(key=lambda x: x["alignment"], reverse=True)
    
    return {
        "best_index": results[0]["index"],
        "best_score": results[0]["alignment"],
        "rankings": results,
        "latency_ms": round((time.perf_counter() - t0) * 1000, 1),
    }

@app.post("/hallucination")
async def hallucination(req: HallucinationRequest):
    t0 = time.perf_counter()
    loop = asyncio.get_event_loop()
    
    q_brain, r_brain = await asyncio.gather(
        loop.run_in_executor(None, tribe.embed_text, req.query),
        loop.run_in_executor(None, tribe.embed_text, req.response),
    )
    
    alignment = tribe.cosine(q_brain, r_brain)
    cload = tribe.cognitive_load(r_brain)
    q_regions = set(tribe.get_dominant_regions(q_brain))
    r_regions = set(tribe.get_dominant_regions(r_brain))
    overlap = q_regions & r_regions
    
    signals = []
    if alignment < req.threshold:
        signals.append("low_alignment")
    if not overlap:
        signals.append("incoherent_regions")
    if cload > 0.85:
        signals.append("extreme_cognitive_load")
    
    is_hallucination = len(signals) >= 2
    
    return {
        "is_hallucination": is_hallucination,
        "confidence": round(1.0 - len(signals) * 0.2, 2),
        "alignment_score": alignment,
        "threshold": req.threshold,
        "signals": signals,
        "latency_ms": round((time.perf_counter() - t0) * 1000, 1),
    }


# ══════════════════════════════════════════════════════════════════════════════
# §4  MAIN
# ══════════════════════════════════════════════════════════════════════════════

def main():
    global tribe, store
    
    parser = argparse.ArgumentParser(description="TRIBE Brain + MSA Minimal Server")
    parser.add_argument("--port", type=int, default=7431)
    parser.add_argument("--host", default="0.0.0.0")
    args = parser.parse_args()
    
    tribe = TribeRunner()
    store = BrainStore()
    
    log.info(f"Démarrage — mode=fallback port={args.port}")
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()