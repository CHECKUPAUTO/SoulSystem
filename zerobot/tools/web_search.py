"""Local CVE/NVD database search tool.

Searches a locally-downloaded and indexed CVE database (via ChromaDB) to
check if a vulnerability is already known. Does NOT make external network
calls — uses a pre-downloaded local CVE dataset.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Any, Optional

from pydantic import BaseModel, Field, PrivateAttr

from langchain_chroma import Chroma
from langchain_huggingface import HuggingFaceEmbeddings  # type: ignore[import-untyped]

from tools.base import BaseAnalysisTool

logger = logging.getLogger(__name__)


class WebSearchInput(BaseModel):
    """Input schema for CVE database search."""

    query: str = Field(
        description="Search query describing the vulnerability or component"
    )
    max_results: int = Field(default=5, description="Maximum number of results to return")


class WebSearchTool(BaseAnalysisTool):
    """Search a local CVE/NVD database for known vulnerabilities.

    Uses a pre-downloaded and ChromaDB-indexed copy of the NVD/CVE dataset.
    No external network calls are made.
    """

    name: str = "web_search"
    description: str = (
        "Search a local CVE/NVD database to check if a vulnerability or "
        "pattern is already known. Provide a description of the vulnerability "
        "(e.g., 'buffer overflow in libX11'). Returns CVE IDs and summaries."
    )
    args_schema: type[BaseModel] = WebSearchInput

    _chromadb_path: str = PrivateAttr(default="/zerobot/data/chromadb")
    _embedding_model: str = PrivateAttr(default="BAAI/bge-large-en-v1.5")
    _vectorstore: Optional[Chroma] = PrivateAttr(default=None)

    def __init__(self, chromadb_path: Optional[str] = None, embedding_model: Optional[str] = None, **kwargs):
        super().__init__(**kwargs)

        # Resolve from config if available
        sandbox_cfg = getattr(self, "container_manager", None)
        if sandbox_cfg and hasattr(sandbox_cfg, "config"):
            config = getattr(sandbox_cfg, "config", {})
            kb_path = config.get("paths", {}).get("chromadb_persist", "/zerobot/data/chromadb")
            model_name = config.get("models", {}).get("embedding", {}).get(
                "model_name", "BAAI/bge-large-en-v1.5"
            )
        else:
            kb_path = chromadb_path or "/zerobot/data/chromadb"
            model_name = embedding_model or "BAAI/bge-large-en-v1.5"

        # Use PrivateAttr via object.__setattr__ to bypass pydantic field validation
        object.__setattr__(self, "_chromadb_path", kb_path)
        object.__setattr__(self, "_embedding_model", model_name)

    def _get_vectorstore(self) -> Optional[Chroma]:
        """Lazy-load the vector store connection."""
        if self._vectorstore is not None:
            return self._vectorstore

        persist_dir = Path(self._chromadb_path)
        if not persist_dir.exists():
            logger.warning(
                "ChromaDB persist directory not found at %s. "
                "Run 'make ingest' first.", self._chromadb_path
            )
            return None

        try:
            embeddings = HuggingFaceEmbeddings(
                model_name=self._embedding_model,
                show_progress=False,
            )
            object.__setattr__(self, "_vectorstore", Chroma(
                persist_directory=self._chromadb_path,
                embedding_function=embeddings,
            ))
            logger.info("Connected to ChromaDB at %s", self._chromadb_path)
        except Exception as e:
            logger.error("Failed to connect to ChromaDB: %s", e)
            object.__setattr__(self, "_vectorstore", None)

        return self._vectorstore

    def _run(self, query: str, max_results: int = 5) -> dict[str, Any]:
        """Search the local CVE database."""
        result: dict[str, Any] = {
            "query": query,
            "success": False,
            "source": "local CVE database",
        }

        vectorstore = self._get_vectorstore()
        if vectorstore is None:
            result["error"] = (
                "CVE database not available. Run the ingestion script first: "
                "python -m knowledge.ingest"
            )
            result["note"] = (
                "You can download the NVD CVE database from "
                "https://nvd.nist.gov/ and index it with the ingest script."
            )
            return result

        try:
            docs = vectorstore.similarity_search_with_relevance_scores(
                query, k=max_results
            )

            if not docs:
                result["results"] = []
                result["total"] = 0
                result["note"] = "No matching CVEs found in local database."
                result["success"] = True
                return result

            cve_results = []
            for doc, score in docs:
                metadata = doc.metadata or {}
                cve_results.append({
                    "cve_id": metadata.get("cve_id", "unknown"),
                    "title": metadata.get("title", ""),
                    "summary": doc.page_content[:500],
                    "score": metadata.get("cvss_score", "N/A"),
                    "severity": metadata.get("severity", "N/A"),
                    "published": metadata.get("published_date", "N/A"),
                    "relevance": round(score, 3),
                })

            result["results"] = cve_results
            result["total"] = len(cve_results)
            result["success"] = True

        except Exception as e:
            result["error"] = f"CVE search failed: {e}"
            logger.error("CVE search error: %s", e, exc_info=True)

        return result

    async def _arun(self, query: str, max_results: int = 5) -> dict[str, Any]:
        """Async variant."""
        return self._run(query, max_results)
