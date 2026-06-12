"""SoulMind HTTP server — resolve + extract + store API.

Exposes a lightweight FastAPI server that wraps the soulmind bridge.
Intended to run alongside SoulSystem (port 9025 by default).

Usage::

    python -m soulmind.server --port 9025
"""

from __future__ import annotations

import argparse
import logging

import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

from soulmind.bridge import (
    ExtractRequest,
    ExtractResponse,
    ResolveRequest,
    ResolveResponse,
    StoreRequest,
    StoreResponse,
    handle_extract,
    handle_resolve,
    handle_search,
    handle_store,
)

logger = logging.getLogger(__name__)


def create_app() -> FastAPI:
    app = FastAPI(
        title="SoulMind",
        version="0.1.0",
        description="Structured knowledge extraction bridge — patterns borrowed from QuantMind (NeurIPS 2025)",
    )

    @app.get("/health")
    async def health():
        return {"ok": True, "version": "0.1.0"}

    @app.post("/api/resolve", response_model=ResolveResponse)
    async def api_resolve(req: ResolveRequest):
        """Natural language → typed intent."""
        return await handle_resolve(req)

    @app.post("/api/extract", response_model=ExtractResponse)
    async def api_extract(req: ExtractRequest):
        """Extract structured knowledge from source."""
        return await handle_extract(req)

    class StoreReq(BaseModel):
        knowledge: dict
        namespace: str = "soulmind"
        ttl_seconds: int | None = None

    @app.post("/api/store", response_model=StoreResponse)
    async def api_store(req: StoreReq):
        """Store extracted knowledge in SoulSystem memory."""
        return await handle_store(
            StoreRequest(knowledge=req.knowledge, namespace=req.namespace, ttl_seconds=req.ttl_seconds)
        )

    class SearchReq(BaseModel):
        query: str
        namespace: str = "soulmind"
        limit: int = 5

    @app.post("/api/search")
    async def api_search(req: SearchReq):
        """Search SoulMind knowledge."""
        return await handle_search(req.query, req.namespace, req.limit)

    return app


def main():
    parser = argparse.ArgumentParser(description="SoulMind server")
    parser.add_argument("--port", type=int, default=9025, help="Port (default: 9025)")
    parser.add_argument("--host", default="127.0.0.1", help="Host (default: 127.0.0.1)")
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO)
    app = create_app()
    logger.info(f"SoulMind starting on {args.host}:{args.port}")
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()
