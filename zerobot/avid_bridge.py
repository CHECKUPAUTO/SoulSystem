"""ZeroBot ↔ SoulSystem Bridge : connexion à la mémoire partagée d'AVID.

Ce module permet à ZeroBot de :
1. STORE : envoyer les résultats d'analyse binaire / crash / exploit
   dans la mémoire partagée de SoulSystem (POST /api/memory/store)
2. SEARCH : enrichir le contexte de l'agent avec les souvenirs stockés
   par AVID et les autres composants (POST /api/memory/search)
3. CONTEXT : récupérer un contexte RAG pré-formaté pour les prompts
   (POST /api/memory/context)

Configuration :
    SOULSYSTEM_URL (env) — défaut "http://soulsystem:9090"
    SOULSYSTEM_TIMEOUT (env) — timeout en secondes, défaut 10s
"""

from __future__ import annotations

import json
import logging
import os
from typing import Any
from urllib.parse import urljoin

import requests

logger = logging.getLogger(__name__)

SOULSYSTEM_URL = os.environ.get("SOULSYSTEM_URL", "http://soulsystem:9090")
SOULSYSTEM_TIMEOUT = int(os.environ.get("SOULSYSTEM_TIMEOUT", "10"))


def _url(path: str) -> str:
    return urljoin(SOULSYSTEM_URL.rstrip("/") + "/", path.lstrip("/"))


# ── Store ─────────────────────────────────────────────────────────

def store_analysis_result(
    binary_name: str,
    analysis_type: str,
    content: str,
    metadata: dict[str, Any] | None = None,
) -> bool:
    """Stocke un résultat d'analyse dans la mémoire partagée.

    Args:
        binary_name: Nom du binaire analysé (tag de provenance).
        analysis_type: Type d'analyse (static, disasm, crash, exploit, fuzz).
        content: Texte du résultat (rapport, backtrace, template, etc.).
        metadata: Métadonnées additionnelles (hash, score, etc.).

    Returns:
        True si stocké, False si erreur.
    """
    meta = metadata or {}
    meta["source"] = "zerobot"
    meta["binary"] = binary_name
    meta["analysis_type"] = analysis_type

    payload = {
        "text": content,
        "metadata": meta,
        "tag": f"zerobot:{analysis_type}:{binary_name}",
    }

    try:
        resp = requests.post(
            _url("/api/memory/store"),
            json=payload,
            timeout=SOULSYSTEM_TIMEOUT,
        )
        if resp.status_code == 200:
            logger.info("Stored %s analysis for '%s' in SoulSystem", analysis_type, binary_name)
            return True
        else:
            logger.warning(
                "SoulSystem store returned %s: %s",
                resp.status_code, resp.text[:200],
            )
            return False
    except requests.RequestException as e:
        logger.warning("SoulSystem store error: %s", e)
        return False


# ── Search ────────────────────────────────────────────────────────

def search_memory(query: str, top_k: int = 5) -> list[dict[str, Any]]:
    """Recherche dans la mémoire partagée (AVID, autres agents).

    Args:
        query: Texte de requête.
        top_k: Nombre max de résultats.

    Returns:
        Liste de hits {text, score, source}.
    """
    try:
        resp = requests.post(
            _url("/api/memory/search"),
            json={"query": query, "top_k": top_k},
            timeout=SOULSYSTEM_TIMEOUT,
        )
        if resp.status_code == 200:
            data = resp.json()
            return data.get("results", [])
        else:
            logger.warning("SoulSystem search returned %s", resp.status_code)
            return []
    except requests.RequestException as e:
        logger.warning("SoulSystem search error: %s", e)
        return []


# ── Context ───────────────────────────────────────────────────────

def get_context(query: str, limit: int = 5) -> str:
    """Récupère un contexte pré-formaté depuis SoulSystem.

    Utile pour enrichir le prompt de l'agent avec le contexte RAG
    partagé (documents, analyses passées, connaissances).
    """
    try:
        resp = requests.post(
            _url("/api/memory/context"),
            json={"query": query, "limit": limit},
            timeout=SOULSYSTEM_TIMEOUT,
        )
        if resp.status_code == 200:
            data = resp.json()
            return data.get("context", "")
        return ""
    except requests.RequestException as e:
        logger.warning("SoulSystem context error: %s", e)
        return ""


# ── Convenience : store all analysis types ────────────────────────

def store_static_analysis(binary_name: str, report: str, sha256: str = "") -> bool:
    """Stocke le résultat d'une analyse statique."""
    return store_analysis_result(
        binary_name, "static", report,
        {"sha256": sha256, "tool": "binary_analysis"},
    )


def store_disassembly(binary_name: str, code: str, function: str = "") -> bool:
    """Stocke le désassemblage d'une fonction."""
    return store_analysis_result(
        binary_name, "disasm", code,
        {"function": function, "tool": "disassemble"},
    )


def store_crash_analysis(binary_name: str, report: str, exploitable: bool = False) -> bool:
    """Stocke un rapport d'analyse de crash."""
    return store_analysis_result(
        binary_name, "crash", report,
        {"exploitable": exploitable, "tool": "crash_analyzer"},
    )


def store_exploit_template(binary_name: str, template: str, exploit_type: str = "") -> bool:
    """Stocke un template d'exploit."""
    return store_analysis_result(
        binary_name, "exploit", template,
        {"exploit_type": exploit_type, "tool": "exploit_templates"},
    )


def store_fuzz_harness(binary_name: str, harness: str) -> bool:
    """Stocke un harness de fuzzing."""
    return store_analysis_result(
        binary_name, "fuzz", harness,
        {"tool": "fuzzing_harness"},
    )
