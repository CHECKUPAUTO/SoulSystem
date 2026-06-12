"""RAG retriever for ZeroBot.

Wraps ChromaDB with LangChain's retrieval pipeline. Returns the top-k
most relevant documents for a given query, filtered by relevance score.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Any, Optional

from langchain_chroma import Chroma
from langchain_huggingface import HuggingFaceEmbeddings  # type: ignore[import-untyped]
from langchain_core.documents import Document
from langchain_core.retrievers import BaseRetriever
from pydantic import Field

logger = logging.getLogger(__name__)


class ZeroBotRetriever(BaseRetriever):
    """Retriever for ZeroBot's vulnerability research knowledge base.

    Uses ChromaDB with BGE embeddings to find the most relevant documents
    for a given query about vulnerabilities, exploitation techniques, or
    security concepts.
    """

    chromadb_path: str = Field(default="/zerobot/data/chromadb")
    embedding_model: str = Field(default="BAAI/bge-large-en-v1.5")
    k: int = Field(default=5)
    score_threshold: float = Field(default=0.5)
    _vectorstore: Optional[Chroma] = None

    model_config = {"arbitrary_types_allowed": True}

    def __init__(
        self,
        chromadb_path: str = "/zerobot/data/chromadb",
        embedding_model: str = "BAAI/bge-large-en-v1.5",
        k: int = 5,
        score_threshold: float = 0.5,
        **kwargs,
    ):
        super().__init__(
            chromadb_path=chromadb_path,
            embedding_model=embedding_model,
            k=k,
            score_threshold=score_threshold,
            **kwargs,
        )
        self._connect()

    def _connect(self) -> None:
        """Initialize the ChromaDB connection."""
        persist_dir = Path(self.chromadb_path)
        if not persist_dir.exists():
            logger.warning(
                "ChromaDB persist directory not found at %s. "
                "Run 'make ingest' first.", self.chromadb_path
            )
            self._vectorstore = None
            return

        try:
            embeddings = HuggingFaceEmbeddings(
                model_name=self.embedding_model,
                show_progress=False,
            )
            self._vectorstore = Chroma(
                persist_directory=self.chromadb_path,
                embedding_function=embeddings,
            )
        except Exception as e:
            logger.error("Failed to connect to ChromaDB: %s", e)
            self._vectorstore = None

    def _get_relevant_documents(self, query: str) -> list[Document]:
        """Retrieve relevant documents for a query.

        Args:
            query: The search query.

        Returns:
            List of relevant Document objects (up to self.k).
        """
        if self._vectorstore is None:
            logger.warning("Vector store not available — returning empty results")
            return []

        try:
            docs_with_scores = self._vectorstore.similarity_search_with_relevance_scores(
                query, k=self.k
            )
            # Filter by score threshold
            filtered = [
                doc for doc, score in docs_with_scores
                if score >= self.score_threshold
            ]
            if not filtered and docs_with_scores:
                # Fallback: return top docs even below threshold
                filtered = [doc for doc, _ in docs_with_scores[:2]]
            return filtered

        except Exception as e:
            logger.error("Retrieval error: %s", e, exc_info=True)
            return []

    def get_relevant_documents_with_scores(
        self, query: str
    ) -> list[tuple[Document, float]]:
        """Retrieve documents with their relevance scores."""
        if self._vectorstore is None:
            return []
        try:
            return self._vectorstore.similarity_search_with_relevance_scores(
                query, k=self.k
            )
        except Exception as e:
            logger.error("Retrieval error: %s", e, exc_info=True)
            return []

    @property
    def is_connected(self) -> bool:
        """Check if the vector store is available."""
        return self._vectorstore is not None
