"""
add_position_encoding = __import__("tribe_position_encoding").add_position_encoding  # add_position_encoding
tribe_msa_server.py
====================
Intégration TRIBE v2 + MSA Memory API — 4 Proposals unifiées
Hardware cible : 64 cœurs CPU / 125 GB RAM / RTX 4060 8 GB

Architecture :
  TRIBE  → CPU FP32 (64 cœurs, ~11 GB RAM)
  MSA    → GPU VRAM (port 7430, déjà actif)
  Ce serveur → port 7431

Proposals implémentées :
  A — Brain-Guided Memory   : /embed, /search
  B — Neural Grounding      : /evaluate, /hallucination
  C — Multimodal RAG        : /index/text, /index/file
  D — Adaptive Context      : /context/adapt

Installation :
  pip install fastapi uvicorn sentence-transformers psutil --break-system-packages
  pip install -e /mnt/nvme/projects/tribev2 --break-system-packages  # si dispo
  python tribe_msa_server.py

  Sans TRIBE (mode dégradé, fonctionne immédiatement) :
  python tribe_msa_server.py --no-tribe

OpenClaw skill :
  cp tribe-brain-skill.js /usr/lib/node_modules/openclaw/skills/
"""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import logging
import math

# Position Encoding Sinusoidal
def add_position_encoding(embeddings, max_len=512):
    """Ajoute position encoding sinusoidal aux embeddings."""
    import math
    seq_len = embeddings.shape[0] if len(embeddings.shape) == 2 else embeddings.shape[1]
    d_model = embeddings.shape[-1]
    
    position = torch.arange(seq_len).unsqueeze(1).float()
    div_term = torch.exp(torch.arange(0, d_model, 2).float() * (-math.log(10000.0) / d_model))
    
    pe = torch.zeros(seq_len, d_model)
    pe[:, 0::2] = torch.sin(position * div_term)
    pe[:, 1::2] = torch.cos(position * div_term)
    
    if isinstance(embeddings, np.ndarray):
        embeddings = torch.from_numpy(embeddings).float()
    
    return embeddings + pe.unsqueeze(0) if embeddings.dim() == 3 else embeddings + pe
import os
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, asdict, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import numpy as np
import requests
import torch
import torch.nn.functional as F
import uvicorn
from fastapi import FastAPI, HTTPException, BackgroundTasks
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
import sys; sys.path.insert(0, "/root"); from tribe_persistence import persistence

# ── Logging ───────────────────────────────────────────────────────────────────
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] TRIBE-MSA — %(message)s",
)
log = logging.getLogger("tribe-msa")

# ── Config ────────────────────────────────────────────────────────────────────
MSA_URL       = os.getenv("MSA_URL",   "http://localhost:7430")
TRIBE_PORT    = int(os.getenv("TRIBE_PORT", "7431"))
N_CPU_THREADS = int(os.getenv("CPU_THREADS", "64"))
N_WORKERS     = int(os.getenv("WORKERS", "8"))
TRIBE_MODEL   = os.getenv("TRIBE_MODEL", "facebook/tribev2")

torch.set_num_threads(N_CPU_THREADS)
torch.set_num_interop_threads(N_WORKERS)

# ══════════════════════════════════════════════════════════════════════════════
# §1  TRIBE RUNNER — CPU FP32 (64 cœurs, ~11 GB RAM)
# ══════════════════════════════════════════════════════════════════════════════

class TribeRunner:
    """
    Charge TRIBE v2 entièrement en RAM CPU.
    Libère la VRAM GPU pour MSA.

    Sans TRIBE : fallback sentence-transformers (1024 dims pseudo-brain).
    """

    N_VERTICES = 20_004   # fsaverage5 standard

    def __init__(self, use_tribe: bool = True):
        self.use_tribe = use_tribe
        self._model    = None
        self._st_model = None
        self._lock     = threading.Lock()
        self._executor = ThreadPoolExecutor(max_workers=N_WORKERS)

        if use_tribe:
            self._load_tribe()
        else:
            self._load_fallback()

    # ── Chargement ────────────────────────────────────────────────────────────

    def _load_tribe(self):
        try:
            from tribev2 import TribeModel
            log.info(f"Chargement TRIBE v2 en CPU FP32 ({N_CPU_THREADS} threads)...")
            self._model = TribeModel.from_pretrained(
                TRIBE_MODEL,
                device_map="cuda:0",
                torch_dtype=torch.float32,
            )
            n_params = sum(p.numel() for p in self._model.parameters()) / 1e9
            log.info(f"TRIBE chargé : {n_params:.2f}B params")
            self._log_ram()
        except ImportError:
            log.warning("TRIBE non installé → fallback sentence-transformers")
            self.use_tribe = False
            self._load_fallback()
        except Exception as e:
            log.error(f"TRIBE échec ({e}) → fallback")
            self.use_tribe = False
            self._load_fallback()

    def _load_fallback(self):
        """
        Charge Snowflake/snowflake-arctic-embed-m via transformers+torch DIRECTEMENT.
        Bypass sentence-transformers qui importe TensorFlow → segfault Python 3.13.
        """
        try:
            # Variable d'environnement préventive (bloque TF dans transformers aussi)
            import os
            os.environ["TRANSFORMERS_NO_TF"] = "1"
            os.environ["TF_CPP_MIN_LOG_LEVEL"] = "3"

            from transformers import AutoTokenizer, AutoModel
            import torch

            model_name = "Snowflake/snowflake-arctic-embed-m"
            log.info(f"Chargement {model_name} via transformers+torch (sans TF)...")
            self._st_tokenizer = AutoTokenizer.from_pretrained(model_name)
            self._st_model = AutoModel.from_pretrained(model_name)
            self._st_model.eval()
            log.info("Fallback transformers+torch prêt (1024 dims, sans TensorFlow)")
        except Exception as e:
            log.warning(f"transformers fallback échoué ({e}) — hash-based embeddings")
            self._st_tokenizer = None
            self._st_model = None

    def _mean_pooling(self, model_output, attention_mask):
        token_emb = model_output[0]
        mask_exp = attention_mask.unsqueeze(-1).expand(token_emb.size()).float()
        return (token_emb * mask_exp).sum(1) / mask_exp.sum(1).clamp(min=1e-9)

    def _log_ram(self):
        try:
            import psutil
            used = psutil.virtual_memory().used / 1e9
            total = psutil.virtual_memory().total / 1e9
            log.info(f"RAM : {used:.1f}/{total:.1f} GB utilisée")
        except ImportError:
            pass

    # ── Embedding ─────────────────────────────────────────────────────────────

    def embed_text(self, text: str) -> np.ndarray:
        """
        Retourne un vecteur brain-like [N_VERTICES].
        TRIBE réel si disponible, sinon projection sentence-transformers.
        """
        with self._lock:
            if self.use_tribe and self._model is not None:
                return self._embed_tribe_text(text)
            else:
                return self._embed_fallback(text)

    def _embed_tribe_text(self, text: str) -> np.ndarray:
        """
        TRIBE réel via text_path (fichier temporaire).
        Contourne le bug get_events_dataframe(text=...) non supporté.
        WhisperX (TTS pipeline) est bypassé — on passe par le LLaMA encoder direct.
        ~2-8s sur 64 cœurs.
        """
        import tempfile, os
        with torch.inference_mode():
            # Écrit le texte dans un fichier temp (TRIBE attend text_path)
            with tempfile.NamedTemporaryFile(
                mode='w', suffix='.txt', delete=False, encoding='utf-8'
            ) as f:
                f.write(text)
                tmp = f.name
            try:
                events = self._model.get_events_dataframe(text_path=tmp)
                preds, _ = self._model.predict(events=events)
                return preds.mean(axis=0).numpy().astype(np.float32)
            except Exception as e:
                # WhisperX conflict ou autre → fallback propre
                log.warning(f"TRIBE text_path échoué ({e}) → fallback")
                return self._embed_fallback(text)
            finally:
                os.unlink(tmp)

    def _embed_fallback(self, text: str) -> np.ndarray:
        """
        Embedding via transformers+torch (sans TensorFlow).
        Projette 384 → N_VERTICES via répétition + bruit déterministe.
        Cohérent : même texte → même vecteur.
        """
        if self._st_model is not None and self._st_tokenizer is not None:
            import torch
            os.environ["TRANSFORMERS_NO_TF"] = "1"
            with torch.no_grad():
                enc = self._st_tokenizer(
                    [text], padding=True, truncation=True,
                    max_length=128, return_tensors="pt"
                )
                out = self._st_model(**enc)
                emb = self._mean_pooling(out, enc["attention_mask"])
                emb = torch.nn.functional.normalize(emb, p=2, dim=1)
                base = emb[0].numpy().astype(np.float32)
        else:
            # Hash déterministe si pas de modèle du tout
            h = int(hashlib.md5(text.encode()).hexdigest(), 16)
            rng = np.random.default_rng(h % (2**32))
            base = rng.standard_normal(384).astype(np.float32)
            base = base / (np.linalg.norm(base) + 1e-8)

        # Projection 384 → N_VERTICES par répétition + permutation déterministe
        repeats   = math.ceil(self.N_VERTICES / len(base))
        expanded  = np.tile(base, repeats)[:self.N_VERTICES]

        # Bruit de texture déterministe (simule la variabilité corticale)
        seed = int(abs(base[0]) * 1e6) % (2**32)
        rng  = np.random.default_rng(seed)
        noise = rng.standard_normal(self.N_VERTICES).astype(np.float32) * 0.05
        result = expanded + noise
        return result / (np.linalg.norm(result) + 1e-8)

    def embed_batch(self, texts: List[str]) -> List[np.ndarray]:
        """Batch parallèle — exploite les 64 cœurs via ThreadPoolExecutor."""
        futures = [self._executor.submit(self.embed_text, t) for t in texts]
        return [f.result() for f in futures]

    def embed_file(self, path: str) -> np.ndarray:
        """Embed le contenu d'un fichier texte."""
        text = Path(path).read_text(encoding="utf-8", errors="ignore")
        # Tronque à 2000 mots pour la latence
        words = text.split()[:2000]
        return self.embed_text(" ".join(words))

    # ── Régions corticales ────────────────────────────────────────────────────

    REGIONS = {
        "visual":      (0,     5_000),
        "auditory":    (5_000, 10_000),
        "prefrontal":  (10_000, 15_000),
        "motor":       (15_000, 20_004),
    }

    def get_dominant_regions(self, pattern: np.ndarray) -> List[str]:
        mean = pattern.mean()
        return [name for name, (s, e) in self.REGIONS.items()
                if pattern[s:e].mean() > mean]

    def cognitive_load(self, pattern: np.ndarray) -> float:
        """Activation préfrontale normalisée [0, 1]."""
        s, e = self.REGIONS["prefrontal"]
        pf   = float(pattern[s:e].mean())
        mn, mx = float(pattern.min()), float(pattern.max())
        if mx == mn: return 0.5
        return round(float(np.clip((pf - mn) / (mx - mn + 1e-8), 0, 1)), 4)

    def cosine(self, a: np.ndarray, b: np.ndarray) -> float:
        na, nb = np.linalg.norm(a), np.linalg.norm(b)
        if na == 0 or nb == 0: return 0.0
        return round(float(np.dot(a, b) / (na * nb)), 4)


# ══════════════════════════════════════════════════════════════════════════════
# §2  BRAIN STORE — Stockage RAM CPU des embeddings (pas en VRAM)
# ══════════════════════════════════════════════════════════════════════════════

@dataclass
class BrainEntry:
    chunk_id:         int
    doc_id:           int
    text_preview:     str          # 200 premiers chars
    brain_pattern:    np.ndarray   # [N_VERTICES] float32
    semantic_vec:     np.ndarray   # [384] float32 (sentence-transformers)
    cognitive_load:   float
    dominant_regions: List[str]
    timestamp:        float        = field(default_factory=time.time)
    metadata:         Dict         = field(default_factory=dict)

class BrainStore:
    """
    Index en RAM CPU des brain embeddings.
    Séparé de MSA (GPU) pour ne pas saturer la VRAM.
    """

    def __init__(self):
        self._entries: Dict[int, BrainEntry] = {}
        self._lock    = threading.Lock()
        self._st      = None
        self._load_st()
        # Charger les entrées persistées
        try:
            saved = persistence.load_entries()
            if saved:
                self._entries = saved
                log.info(f"Loaded {len(self._entries)} persisted entries")
        except Exception as e:
            log.warning(f"Could not load persisted entries: {e}")

    def _load_st(self):
        try:
            from sentence_transformers import SentenceTransformer
            self._st = SentenceTransformer("Snowflake/snowflake-arctic-embed-m")
        except ImportError:
            pass

    def add(self, entry: BrainEntry):
        with self._lock:
            self._entries[entry.chunk_id] = entry
        # Sauvegarder sur disque
        try:
            persistence.save_entries(self._entries)
        except Exception as e:
            log.warning(f"Could not save entries: {e}")

    def get(self, chunk_id: int) -> Optional[BrainEntry]:
        return self._entries.get(chunk_id)

    def all_ids(self) -> List[int]:
        return list(self._entries.keys())

    def semantic_embed(self, text: str) -> np.ndarray:
        if self._st:
            return self._st.encode(text, normalize_embeddings=True)
        h   = int(hashlib.md5(text.encode()).hexdigest(), 16)
        rng = np.random.default_rng(h % (2**32))
        v   = rng.standard_normal(384).astype(np.float32)
        return v / (np.linalg.norm(v) + 1e-8)

    def search(
        self,
        query_brain: np.ndarray,
        query_semantic: np.ndarray,
        top_k: int = 10,
        brain_weight: float = 0.3,
        max_cognitive_load: float = 1.0,
    ) -> List[Dict]:
        """
        Recherche hybride : sémantique (0.7) + brain (0.3).
        Filtre par cognitive_load si max_cognitive_load < 1.
        """
        results = []
        with self._lock:
            entries = list(self._entries.values())

        for e in entries:
            if e.cognitive_load > max_cognitive_load:
                continue
            sem   = float(np.dot(query_semantic, e.semantic_vec) /
                         (np.linalg.norm(query_semantic)*np.linalg.norm(e.semantic_vec)+1e-8))
            brain = float(np.dot(query_brain, e.brain_pattern) /
                         (np.linalg.norm(query_brain)*np.linalg.norm(e.brain_pattern)+1e-8))
            score = (1 - brain_weight) * sem + brain_weight * brain
            results.append({
                "chunk_id":       e.chunk_id,
                "doc_id":         e.doc_id,
                "text_preview":   e.text_preview,
                "semantic_score": round(sem,   4),
                "brain_score":    round(brain, 4),
                "combined_score": round(score, 4),
                "cognitive_load": e.cognitive_load,
                "dominant_regions": e.dominant_regions,
                "metadata":       e.metadata,
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
                "n_entries":        n,
                "avg_cognitive_load": round(sum(loads)/n, 3),
                "min_load":         round(min(loads), 3),
                "max_load":         round(max(loads), 3),
                "ram_mb":           round(
                    sum(e.brain_pattern.nbytes + e.semantic_vec.nbytes
                        for e in self._entries.values()) / 1e6, 1),
            }


# ══════════════════════════════════════════════════════════════════════════════
# §3  ÉTAT GLOBAL
# ══════════════════════════════════════════════════════════════════════════════

class ServerState:
    def __init__(self, use_tribe: bool):
        self.tribe  = TribeRunner(use_tribe=use_tribe)
        self.store  = BrainStore()
        self._ctr   = 0
        self._lock  = threading.Lock()

    def next_id(self) -> int:
        with self._lock:
            self._ctr += 1
            return self._ctr

    def msa_compress(self, values: List[float], doc_id: int) -> Optional[Dict]:
        """Envoie vers MSA API (port 7430)."""
        try:
            r = requests.post(f"{MSA_URL}/compress",
                              json={"values": values, "doc_id": doc_id},
                              timeout=30)
            return r.json() if r.ok else None
        except Exception as e:
            log.warning(f"MSA compress échoué : {e}")
            return None

    def msa_search(self, query_emb: List[float], top_k: int = 10) -> List[Dict]:
        """Recherche MSA si endpoint /search disponible."""
        try:
            r = requests.post(f"{MSA_URL}/search",
                              json={"values": query_emb, "top_k": top_k},
                              timeout=10)
            return r.json().get("results", []) if r.ok else []
        except Exception:
            return []


state: Optional[ServerState] = None


# ══════════════════════════════════════════════════════════════════════════════
# §4  FASTAPI
# ══════════════════════════════════════════════════════════════════════════════

app = FastAPI(
    title="TRIBE + MSA Brain API",
    description="Brain-guided memory, evaluation, RAG et adaptive context",
    version="1.0",
)
app.add_middleware(CORSMiddleware, allow_origins=["*"],
                   allow_methods=["*"], allow_headers=["*"])

# ── Modèles Pydantic ──────────────────────────────────────────────────────────

class EmbedRequest(BaseModel):
    text:               str
    doc_id:             int   = 0
    compress_to_msa:    bool  = True
    metadata:           Dict  = {}

class EmbedResponse(BaseModel):
    chunk_id:         int
    cognitive_load:   float
    dominant_regions: List[str]
    brain_dims:       int
    msa_compression:  Optional[float]
    msa_snr_db:       Optional[float]
    latency_ms:       float
    mode:             str    # "tribe" ou "fallback"

class SearchRequest(BaseModel):
    query:              str
    top_k:              int   = 10
    brain_weight:       float = 0.3
    max_cognitive_load: float = 1.0

class IndexFileRequest(BaseModel):
    path:            str
    doc_id:          int  = 0
    compress_to_msa: bool = True
    metadata:        Dict = {}

class EvaluateRequest(BaseModel):
    query:      str
    candidates: List[str]
    reference:  Optional[str] = None

class HallucinationRequest(BaseModel):
    query:     str
    response:  str
    threshold: float = 0.35

class AdaptContextRequest(BaseModel):
    query:              str
    documents:          List[str]
    max_cognitive_load: float = 0.7
    max_tokens:         int   = 4000

# ── Routes ────────────────────────────────────────────────────────────────────

@app.get("/health")
async def health():
    import psutil
    ram = psutil.virtual_memory()
    return {
        "status":         "ok",
        "tribe_mode":     "tribe" if state.tribe.use_tribe else "fallback",
        "brain_entries":  len(state.store.all_ids()),
        "ram_used_gb":    round(ram.used / 1e9, 1),
        "ram_total_gb":   round(ram.total / 1e9, 1),
        "cpu_threads":    N_CPU_THREADS,
        "msa_url":        MSA_URL,
    }

@app.get("/info")
async def info():
    return {
        **await health(),
        "store_stats": state.store.stats(),
        "proposals": {
            "A_brain_memory":     "/embed  /search",
            "B_neural_grounding": "/evaluate  /hallucination",
            "C_multimodal_rag":   "/index/text  /index/file",
            "D_adaptive_context": "/context/adapt",
        },
    }

# ── Proposal A : Brain-Guided Memory ─────────────────────────────────────────

@app.post("/embed", response_model=EmbedResponse)
async def embed(req: EmbedRequest, bg: BackgroundTasks):
    """
    [Proposal A] Génère un brain embedding et stocke dans BrainStore + MSA.
    CPU TRIBE (~2-8s) + GPU MSA (<1s) en parallèle.
    """
    t0 = time.perf_counter()

    loop = asyncio.get_event_loop()
    brain = await loop.run_in_executor(None, state.tribe.embed_text, req.text)

    cload   = state.tribe.cognitive_load(brain)
    regions = state.tribe.get_dominant_regions(brain)
    sem_vec = state.store.semantic_embed(req.text)
    cid     = state.next_id()

    # Stockage BrainStore (RAM CPU)
    entry = BrainEntry(
        chunk_id=cid, doc_id=req.doc_id,
        text_preview=req.text[:200],
        brain_pattern=brain, semantic_vec=sem_vec,
        cognitive_load=cload, dominant_regions=regions,
        metadata=req.metadata,
    )
    state.store.add(entry)

    # Compression MSA optionnelle (async en arrière-plan)
    msa_result = None
    if req.compress_to_msa:
        # Compresse le brain embedding int3 via MSA
        brain_list = brain.tolist()
        msa_result = await loop.run_in_executor(
            None, state.msa_compress, brain_list, req.doc_id)

    latency = (time.perf_counter() - t0) * 1000
    return EmbedResponse(
        chunk_id=cid,
        cognitive_load=cload,
        dominant_regions=regions,
        brain_dims=len(brain),
        msa_compression=msa_result.get("compression_vs_fp32") if msa_result else None,
        msa_snr_db=msa_result.get("snr_db") if msa_result else None,
        latency_ms=round(latency, 1),
        mode="tribe" if state.tribe.use_tribe else "fallback",
    )


@app.post("/search")
async def search(req: SearchRequest):
    """
    [Proposal A] Recherche hybride sémantique + brain-weighted.
    Filtre par cognitive_load si max_cognitive_load < 1.
    """
    t0 = time.perf_counter()
    loop = asyncio.get_event_loop()

    # Embed la requête en parallèle
    brain_q, sem_q = await asyncio.gather(
        loop.run_in_executor(None, state.tribe.embed_text, req.query),
        loop.run_in_executor(None, state.store.semantic_embed, req.query),
    )

    results = state.store.search(
        brain_q, sem_q,
        top_k=req.top_k,
        brain_weight=req.brain_weight,
        max_cognitive_load=req.max_cognitive_load,
    )

    return {
        "query":        req.query,
        "n_results":    len(results),
        "brain_weight": req.brain_weight,
        "results":      results,
        "latency_ms":   round((time.perf_counter()-t0)*1000, 1),
    }

# ── Proposal B : Neural Grounding ────────────────────────────────────────────

@app.post("/evaluate")
async def evaluate(req: EvaluateRequest):
    """
    [Proposal B] Évalue des réponses candidates par cosine similarity brain.
    Le meilleur candidat = celui dont le brain pattern se rapproche le plus
    du pattern query (+ optionnellement de la référence).
    """
    t0   = time.perf_counter()
    loop = asyncio.get_event_loop()

    # Embed query + tous les candidats en parallèle
    all_texts   = [req.query] + req.candidates
    if req.reference:
        all_texts.append(req.reference)

    embeddings = await loop.run_in_executor(
        None, state.tribe.embed_batch, all_texts)

    query_brain = embeddings[0]
    cand_brains = embeddings[1:len(req.candidates)+1]
    ref_brain   = embeddings[-1] if req.reference else None

    results = []
    for i, (cand, cb) in enumerate(zip(req.candidates, cand_brains)):
        # Similarité query ↔ candidat
        q_sim = state.tribe.cosine(query_brain, cb)
        # Similarité référence ↔ candidat (si dispo)
        r_sim = state.tribe.cosine(ref_brain, cb) if ref_brain is not None else None
        # Score final
        score = r_sim if r_sim is not None else q_sim

        results.append({
            "index":          i,
            "candidate":      cand[:200],
            "query_alignment":    q_sim,
            "reference_alignment": r_sim,
            "final_score":    score,
            "cognitive_load": state.tribe.cognitive_load(cb),
            "dominant_regions": state.tribe.get_dominant_regions(cb),
        })

    results.sort(key=lambda x: x["final_score"], reverse=True)

    return {
        "best_index":   results[0]["index"],
        "best_score":   results[0]["final_score"],
        "rankings":     results,
        "latency_ms":   round((time.perf_counter()-t0)*1000, 1),
    }


@app.post("/hallucination")
async def detect_hallucination(req: HallucinationRequest):
    """
    [Proposal B] Détecte les hallucinations potentielles.
    Heuristique : faible alignement brain query↔response = signal d'hallucination.

    Note : heuristique, pas une vérité absolue.
    Valider sur ton domaine avant usage en production.
    """
    t0   = time.perf_counter()
    loop = asyncio.get_event_loop()

    q_brain, r_brain = await asyncio.gather(
        loop.run_in_executor(None, state.tribe.embed_text, req.query),
        loop.run_in_executor(None, state.tribe.embed_text, req.response),
    )

    alignment   = state.tribe.cosine(q_brain, r_brain)
    cload       = state.tribe.cognitive_load(r_brain)
    q_regions   = set(state.tribe.get_dominant_regions(q_brain))
    r_regions   = set(state.tribe.get_dominant_regions(r_brain))
    overlap     = q_regions & r_regions

    signals = []
    if alignment < req.threshold:
        signals.append("low_alignment")
    if not overlap:
        signals.append("incoherent_regions")
    if cload > 0.85:
        signals.append("extreme_cognitive_load")

    is_hallucination = len(signals) >= 2
    confidence       = round(1.0 - len(signals) * 0.2, 2)

    return {
        "is_hallucination":    is_hallucination,
        "confidence":          max(0.0, confidence),
        "alignment_score":     alignment,
        "threshold":           req.threshold,
        "signals":             signals,
        "query_regions":       list(q_regions),
        "response_regions":    list(r_regions),
        "region_overlap":      list(overlap),
        "response_cog_load":   cload,
        "latency_ms":          round((time.perf_counter()-t0)*1000, 1),
    }

# ── Proposal C : Multimodal RAG ───────────────────────────────────────────────

@app.post("/index/text")
async def index_text(req: EmbedRequest):
    """
    [Proposal C] Indexe un texte — alias de /embed avec métadonnées RAG.
    Retourne chunk_id utilisable pour /search ultérieur.
    """
    req.metadata["type"] = "text"
    return await embed(req, BackgroundTasks())


@app.post("/index/file")
async def index_file(req: IndexFileRequest):
    """
    [Proposal C] Indexe un fichier texte depuis le disque.
    Découpe en chunks de 500 mots si > 1000 mots.
    Compatible : .txt, .md, .py, .json, .csv et tout fichier texte.
    """
    t0   = time.perf_counter()
    path = Path(req.path)
    if not path.exists():
        raise HTTPException(404, f"Fichier introuvable : {req.path}")

    text  = path.read_text(encoding="utf-8", errors="ignore")
    words = text.split()
    log.info(f"Indexation {path.name} : {len(words)} mots")

    # Découpage en chunks
    CHUNK_WORDS = 500
    if len(words) <= CHUNK_WORDS:
        chunks = [text]
    else:
        chunks = []
        for i in range(0, len(words), CHUNK_WORDS):
            chunks.append(" ".join(words[i:i+CHUNK_WORDS]))

    log.info(f"  {len(chunks)} chunk(s) à indexer")

    # Embed tous les chunks en parallèle (CPU)
    loop       = asyncio.get_event_loop()
    brains     = await loop.run_in_executor(
        None, state.tribe.embed_batch, chunks)

    chunk_ids  = []
    for i, (chunk, brain) in enumerate(zip(chunks, brains)):
        cid     = state.next_id()
        cload   = state.tribe.cognitive_load(brain)
        regions = state.tribe.get_dominant_regions(brain)
        sem_vec = state.store.semantic_embed(chunk)

        entry = BrainEntry(
            chunk_id=cid, doc_id=req.doc_id,
            text_preview=chunk[:200],
            brain_pattern=brain, semantic_vec=sem_vec,
            cognitive_load=cload, dominant_regions=regions,
            metadata={
                "type": "file", "path": str(path),
                "chunk_idx": i, "n_chunks": len(chunks),
                **req.metadata,
            },
        )
        state.store.add(entry)

        if req.compress_to_msa:
            await loop.run_in_executor(
                None, state.msa_compress, brain.tolist(), req.doc_id)

        chunk_ids.append(cid)

    return {
        "file":       str(path),
        "n_chunks":   len(chunks),
        "chunk_ids":  chunk_ids,
        "total_words": len(words),
        "latency_ms": round((time.perf_counter()-t0)*1000, 1),
    }


@app.get("/index/list")
async def list_index():
    """[Proposal C] Liste tous les documents indexés."""
    with state.store._lock:
        entries = list(state.store._entries.values())

    docs: Dict[int, Dict] = {}
    for e in entries:
        did = e.doc_id
        if did not in docs:
            docs[did] = {"doc_id": did, "chunks": [], "type": e.metadata.get("type","text")}
        docs[did]["chunks"].append({
            "chunk_id":       e.chunk_id,
            "preview":        e.text_preview[:80],
            "cognitive_load": e.cognitive_load,
            "regions":        e.dominant_regions,
        })

    return {
        "n_documents": len(docs),
        "n_chunks":    len(entries),
        "documents":   list(docs.values()),
        "store_stats": state.store.stats(),
    }

# ── Proposal D : Adaptive Context ────────────────────────────────────────────

@app.post("/context/adapt")
async def adapt_context(req: AdaptContextRequest):
    """
    [Proposal D] Adapte la liste de documents au cognitive load de la requête.

    Filtre les documents trop complexes, ordonne par pertinence brain,
    tronque à max_tokens.
    """
    t0   = time.perf_counter()
    loop = asyncio.get_event_loop()

    # Cognitive load de la requête
    q_brain = await loop.run_in_executor(
        None, state.tribe.embed_text, req.query)
    q_load  = state.tribe.cognitive_load(q_brain)
    q_sem   = state.store.semantic_embed(req.query)

    adapted = []
    total_words = 0

    # Embed tous les documents pour le brain scoring
    doc_brains = await loop.run_in_executor(
        None, state.tribe.embed_batch, req.documents)

    scored = []
    for doc, db in zip(req.documents, doc_brains):
        doc_load  = state.tribe.cognitive_load(db)
        brain_sim = state.tribe.cosine(q_brain, db)
        doc_sem   = state.store.semantic_embed(doc)
        sem_sim   = float(np.dot(q_sem, doc_sem) /
                         (np.linalg.norm(q_sem)*np.linalg.norm(doc_sem)+1e-8))
        scored.append((doc, doc_load, brain_sim*0.3 + sem_sim*0.7))

    # Trie par relevance
    scored.sort(key=lambda x: x[2], reverse=True)

    for doc, dload, score in scored:
        # Filtre par complexité cognitive
        if dload > req.max_cognitive_load + 0.15:
            continue
        words = len(doc.split())
        # Approx tokens : mots × 1.3
        if total_words + words * 1.3 > req.max_tokens:
            break
        adapted.append({
            "text":           doc,
            "cognitive_load": round(dload, 3),
            "relevance":      round(score, 4),
        })
        total_words += words

    # Config recommandée selon le load
    if q_load < 0.35:
        depth = "high"
    elif q_load < 0.65:
        depth = "medium"
    else:
        depth = "low"

    return {
        "query_cognitive_load": q_load,
        "technical_depth":      depth,
        "n_docs_in":           len(req.documents),
        "n_docs_out":          len(adapted),
        "estimated_tokens":    round(total_words * 1.3),
        "documents":           adapted,
        "latency_ms":          round((time.perf_counter()-t0)*1000, 1),
    }


# ── Endpoint batch pour l'agent ───────────────────────────────────────────────

@app.post("/agent/store_and_search")
async def agent_store_and_search(body: Dict[str, Any]):
    """
    Endpoint combiné pour OpenClaw :
    1. Indexe les documents fournis
    2. Recherche avec la query
    3. Retourne les résultats adaptés au cognitive load

    Body :
      query      : str
      documents  : List[str]   (à indexer si pas encore fait)
      doc_ids    : List[int]   (optionnel)
      top_k      : int = 5
      brain_w    : float = 0.3
    """
    query = body.get("query", "")
    docs  = body.get("documents", [])
    top_k = body.get("top_k", 5)
    bw    = body.get("brain_w", 0.3)

    # Indexation rapide si docs fournis
    if docs:
        loop   = asyncio.get_event_loop()
        brains = await loop.run_in_executor(None, state.tribe.embed_batch, docs)
        for i, (doc, brain) in enumerate(zip(docs, brains)):
            cid  = state.next_id()
            did  = body.get("doc_ids", [0]*len(docs))[i] if i < len(body.get("doc_ids",[])) else 0
            entry = BrainEntry(
                chunk_id=cid, doc_id=did,
                text_preview=doc[:200],
                brain_pattern=brain,
                semantic_vec=state.store.semantic_embed(doc),
                cognitive_load=state.tribe.cognitive_load(brain),
                dominant_regions=state.tribe.get_dominant_regions(brain),
            )
            state.store.add(entry)

    # Recherche
    search_req = SearchRequest(query=query, top_k=top_k, brain_weight=bw)
    return await search(search_req)


# ══════════════════════════════════════════════════════════════════════════════
# §5  MAIN
# ══════════════════════════════════════════════════════════════════════════════

def main():
    global state

    parser = argparse.ArgumentParser(description="TRIBE Brain + MSA Integration Server")
    parser.add_argument("--no-tribe", action="store_true",
                        help="Mode fallback sentence-transformers (sans TRIBE)")
    parser.add_argument("--port",     type=int, default=TRIBE_PORT)
    parser.add_argument("--host",     default="0.0.0.0")
    args = parser.parse_args()

    use_tribe = not args.no_tribe
    log.info(f"Démarrage — mode={'TRIBE CPU' if use_tribe else 'FALLBACK'} "
             f"port={args.port} threads={N_CPU_THREADS}")

    state = ServerState(use_tribe=use_tribe)
    log.info(f"Swagger : http://localhost:{args.port}/docs")

    uvicorn.run(app, host=args.host, port=args.port, log_level="info", workers=1)


if __name__ == "__main__":
    main()