"""SoulMind bridge — connect quant-mind patterns to SoulSystem.

This module:
1. Exposes ``POST /api/soulmind/resolve`` — natural language → typed intent
2. Exposes ``POST /api/soulmind/extract`` — extract knowledge from text/URL
3. Stores results in SoulSystem memory organ via the existing API

Usage from SoulSystem::

    POST http://localhost:9023/api/soulmind/resolve
    {"text": "arXiv paper 2401.12345 about momentum", "target": "SourceRef"}

Returns::

    {"ok": true, "result": {"kind": "arxiv", "uri": "2401.12345", ...}}
"""

from __future__ import annotations

import json
import os
import asyncio
from typing import Any

import httpx
from pydantic import BaseModel, Field

from soulmind.knowledge import SourceRef, BaseKnowledge, TreeKnowledge, TreeNode
from soulmind.magic import resolve

# ── Config ───────────────────────────────────────────────────────────

SOULSYSTEM_URL = os.environ.get("SOULSYSTEM_URL", "http://localhost:9023")
SOULSYSTEM_TIMEOUT = int(os.environ.get("SOULSYSTEM_TIMEOUT", "30"))


# ── Models ───────────────────────────────────────────────────────────

class ResolveRequest(BaseModel):
    text: str
    target: str = Field(default="SourceRef", description="Pydantic model name to resolve to")
    model: str = Field(default="gpt-4o-mini")
    extra_instructions: str | None = None


class ResolveResponse(BaseModel):
    ok: bool
    result: dict[str, Any] | None = None
    error: str | None = None


class ExtractRequest(BaseModel):
    source: str  # URL or raw text
    source_kind: str = Field(default="http", description="arxiv, http, local, manual")
    model: str = Field(default="gpt-4o-mini")
    extract_type: str = Field(default="tree", description="tree or flat")


class ExtractResponse(BaseModel):
    ok: bool
    knowledge: Any | None = None
    error: str | None = None


class StoreRequest(BaseModel):
    """Store extracted knowledge in SoulSystem memory."""
    knowledge: dict[str, Any]
    namespace: str = Field(default="soulmind")
    ttl_seconds: int | None = None


class StoreResponse(BaseModel):
    ok: bool
    memory_id: str | None = None
    error: str | None = None


# ── Resolver ─────────────────────────────────────────────────────────

TARGET_REGISTRY: dict[str, type[BaseModel]] = {
    "SourceRef": SourceRef,
    # Add more as needed
}


async def handle_resolve(req: ResolveRequest) -> ResolveResponse:
    """Resolve natural language text into a typed Pydantic model."""
    target_type = TARGET_REGISTRY.get(req.target)
    if target_type is None:
        return ResolveResponse(
            ok=False,
            error=f"Unknown target type '{req.target}'. Known: {list(TARGET_REGISTRY)}",
        )
    try:
        obj = await resolve(
            req.text,
            target_type,
            model=req.model,
            extra_instructions=req.extra_instructions,
        )
        return ResolveResponse(ok=True, result=obj.model_dump(mode="json"))
    except Exception as e:
        return ResolveResponse(ok=False, error=str(e))


# ── Extract ──────────────────────────────────────────────────────────

_EXTRACT_TREE_PROMPT = """\
You extract unstructured text into a ``TreeKnowledge`` hierarchy.

Rules:
- Create a root node summarizing the whole document.
- Break the content into logical sections as child nodes.
- Each node needs: title, summary, and content (the relevant markdown).
- Include citations for key claims (quote the source text).
- Set confidence to "high" only for explicit statements, "medium" for implications.

Output schema:
{
  "title": "...",
  "summary": "...",
  "item_type": "extraction",
  "confidence": "medium",
  "source_kind": "{source_kind}",
  "source_uri": "{source_uri}",
  "tree": {
    "title": "...",
    "summary": "...",
    "children": [
      {"title": "...", "summary": "...", "content": "...", "citations": [...]}
    ]
  },
  "tags": ["..."]
}
"""


async def handle_extract(req: ExtractRequest) -> ExtractResponse:
    """Extract structured knowledge from a source."""
    try:
        content = await _fetch_content(req.source, req.source_kind)
        instructions = _EXTRACT_TREE_PROMPT.format(
            source_kind=req.source_kind,
            source_uri=req.source,
        )

        from soulmind.magic import _client as get_client

        client = get_client(None, None)
        response = client.chat.completions.create(
            model=req.model,
            messages=[
                {"role": "system", "content": instructions},
                {"role": "user", "content": content[:80000]},  # truncate
            ],
            temperature=0,
            response_format={"type": "json_object"},
        )
        raw = response.choices[0].message.content
        if not raw:
            return ExtractResponse(ok=False, error="LLM returned empty")

        data = json.loads(raw)

        # Build TreeKnowledge
        def _build_node(d: dict) -> TreeNode:
            children = [_build_node(c) for c in d.get("children", [])]
            return TreeNode(
                title=d.get("title", "Untitled"),
                summary=d.get("summary", ""),
                content=d.get("content", ""),
                children=children,
                citations=d.get("citations", []),
                metadata=d.get("metadata", {}),
            )

        tree_root = _build_node(data.get("tree", {"title": "Root", "summary": ""}))
        source_ref = SourceRef(kind=req.source_kind, uri=req.source)
        knowledge = TreeKnowledge(
            item_type="extraction",
            source=source_ref,
            root=tree_root,
            as_of=data.get("as_of"),
            confidence=data.get("confidence", "medium"),
            tags=data.get("tags", []),
            summary=data.get("summary", tree_root.summary),
        )

        return ExtractResponse(ok=True, knowledge=knowledge.model_dump(mode="json"))
    except Exception as e:
        return ExtractResponse(ok=False, error=str(e))


# ── Store ────────────────────────────────────────────────────────────

async def handle_store(req: StoreRequest) -> StoreResponse:
    """Store knowledge in SoulSystem memory organ."""
    async with httpx.AsyncClient(timeout=SOULSYSTEM_TIMEOUT) as client:
        try:
            resp = await client.post(
                f"{SOULSYSTEM_URL}/api/memory/store",
                json={
                    "namespace": req.namespace,
                    "data": req.knowledge,
                    "ttl_seconds": req.ttl_seconds,
                },
            )
            resp.raise_for_status()
            data = resp.json()
            return StoreResponse(ok=True, memory_id=data.get("id"))
        except Exception as e:
            return StoreResponse(ok=False, error=str(e))


async def handle_search(query: str, namespace: str = "soulmind", limit: int = 5) -> dict[str, Any]:
    """Search SoulSystem memory for relevant knowledge."""
    async with httpx.AsyncClient(timeout=SOULSYSTEM_TIMEOUT) as client:
        resp = await client.post(
            f"{SOULSYSTEM_URL}/api/memory/search",
            json={"query": query, "namespace": namespace, "limit": limit},
        )
        resp.raise_for_status()
        return resp.json()


# ── HTTP helpers ─────────────────────────────────────────────────────

async def _fetch_content(source: str, kind: str) -> str:
    """Fetch content from various source kinds."""
    if kind == "manual":
        return source  # Already raw text

    import asyncio

    if kind in ("http", "arxiv"):
        async with httpx.AsyncClient(timeout=30, follow_redirects=True) as client:
            resp = await client.get(source)
            resp.raise_for_status()
            return resp.text

    if kind == "local":
        return await asyncio.to_thread(lambda: open(source).read())

    raise ValueError(f"Unknown source kind: {kind}")
