"""Document ingestion script for ZeroBot's knowledge base.

Usage:
    python -m knowledge.ingest                      # Ingest default directory
    python -m knowledge.ingest --source /path/to/docs
    python -m knowledge.ingest --rebuild             # Clear and re-ingest

Walks through the document directory, chunks text, and indexes into ChromaDB.
Supported formats: .txt, .md, .pdf, .json (CVE records)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import logging
import os
import re
import sys
from pathlib import Path
from typing import Any, Optional

from langchain.text_splitter import RecursiveCharacterTextSplitter
from langchain_chroma import Chroma
from langchain_huggingface import HuggingFaceEmbeddings  # type: ignore[import-untyped]
from langchain_core.documents import Document

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger(__name__)


def load_text_file(path: Path) -> list[Document]:
    """Load a text or markdown file as a Document."""
    try:
        content = path.read_text(encoding="utf-8", errors="replace")
        metadata = {
            "source": str(path),
            "type": path.suffix.lstrip("."),
            "filename": path.name,
        }
        return [Document(page_content=content, metadata=metadata)]
    except Exception as e:
        logger.warning("Failed to load %s: %s", path, e)
        return []


def load_json_cve(path: Path) -> list[Document]:
    """Load a JSON file containing CVE records."""
    docs: list[Document] = []
    try:
        data = json.loads(path.read_text(encoding="utf-8", errors="replace"))

        # Handle both list-of-records and single-record formats
        records = data if isinstance(data, list) else [data]

        for record in records:
            cve_id = record.get("id") or record.get("cve_id") or record.get("CVE_ID", "")
            description = record.get("description") or record.get("summary", "")
            if isinstance(description, dict):
                description = description.get("value", json.dumps(description))

            if not cve_id or not description:
                continue

            metadata = {
                "cve_id": cve_id,
                "source": str(path),
                "type": "cve",
                "cvss_score": record.get("cvss", record.get("cvss_score", "N/A")),
                "severity": record.get("severity", record.get("baseSeverity", "N/A")),
                "published_date": record.get("published", record.get("publishedDate", "")),
                "title": description[:200],
            }

            docs.append(Document(page_content=str(description), metadata=metadata))
    except Exception as e:
        logger.warning("Failed to load CVE JSON %s: %s", path, e)

    return docs


def load_documents(source_dir: str | Path) -> list[Document]:
    """Recursively load all supported documents from a directory."""
    source_path = Path(source_dir)
    if not source_path.exists():
        logger.error("Source directory not found: %s", source_dir)
        return []

    docs: list[Document] = []
    extensions = {
        ".txt": load_text_file,
        ".md": load_text_file,
        ".json": load_json_cve,
        ".pdf": _load_pdf_fallback,
    }

    for ext, loader in extensions.items():
        for file_path in source_path.rglob(f"*{ext}"):
            logger.info("Loading %s ...", file_path)
            try:
                file_docs = loader(file_path)
                docs.extend(file_docs)
                logger.info("  → %d documents from %s", len(file_docs), file_path.name)
            except Exception as e:
                logger.error("Error loading %s: %s", file_path, e)

    return docs


def _load_pdf_fallback(path: Path) -> list[Document]:
    """Load a PDF file if PyMuPDF is available, otherwise warn."""
    try:
        import fitz  # PyMuPDF
    except ImportError:
        logger.warning(
            "PyMuPDF not installed. Skip PDF: %s. "
            "Install with: pip install pymupdf", path
        )
        return []

    docs: list[Document] = []
    try:
        doc = fitz.open(str(path))
        text = ""
        for page_num, page in enumerate(doc):
            text += f"\n--- Page {page_num + 1} ---\n"
            text += page.get_text()

        metadata = {
            "source": str(path),
            "type": "pdf",
            "filename": path.name,
            "pages": doc.page_count,
        }
        docs.append(Document(page_content=text, metadata=metadata))
        doc.close()
    except Exception as e:
        logger.warning("Failed to parse PDF %s: %s", path, e)

    return docs


def chunk_documents(
    documents: list[Document],
    chunk_size: int = 512,
    chunk_overlap: int = 64,
) -> list[Document]:
    """Split documents into overlapping chunks."""
    splitter = RecursiveCharacterTextSplitter(
        chunk_size=chunk_size,
        chunk_overlap=chunk_overlap,
        separators=["\n\n", "\n", ". ", " ", ""],
        length_function=len,
    )
    return splitter.split_documents(documents)


def ingest(
    source_dir: str | Path = "knowledge/documents",
    persist_dir: str | Path = "/zerobot/data/chromadb",
    embedding_model: str = "BAAI/bge-large-en-v1.5",
    chunk_size: int = 512,
    chunk_overlap: int = 64,
    rebuild: bool = False,
) -> int:
    """Ingest documents into ChromaDB."""
    persist_path = Path(persist_dir)

    # Handle rebuild: clear existing index
    if rebuild and persist_path.exists():
        logger.warning("Rebuild requested — removing existing index at %s", persist_path)
        import shutil
        shutil.rmtree(persist_path)

    # Create persist directory
    persist_path.mkdir(parents=True, exist_ok=True)

    # Load documents
    logger.info("Loading documents from %s ...", source_dir)
    docs = load_documents(source_dir)
    logger.info("Loaded %d raw documents", len(docs))

    if not docs:
        logger.warning("No documents found. Add files to %s", source_dir)
        return 0

    # Chunk
    chunked = chunk_documents(docs, chunk_size, chunk_overlap)
    logger.info("Chunked into %d segments", len(chunked))

    # Create embeddings and index
    logger.info("Creating embeddings with %s ...", embedding_model)
    embeddings = HuggingFaceEmbeddings(
        model_name=embedding_model,
        show_progress=True,
    )

    # Deduplicate by content hash
    seen_hashes: set[str] = set()
    unique_chunks: list[Document] = []
    for doc in chunked:
        h = hashlib.md5(doc.page_content.encode()).hexdigest()
        if h not in seen_hashes:
            seen_hashes.add(h)
            unique_chunks.append(doc)

    logger.info("Indexing %d unique chunks into ChromaDB ...", len(unique_chunks))

    _ = Chroma.from_documents(
        documents=unique_chunks,
        embedding=embeddings,
        persist_directory=str(persist_path),
    )

    # ChromaDB v0.5 auto-persists
    logger.info("Ingestion complete — %d documents indexed at %s", len(unique_chunks), persist_path)
    return len(unique_chunks)


def main():
    """CLI entry point."""
    parser = argparse.ArgumentParser(description="ZeroBot Knowledge Base Ingestion")
    parser.add_argument(
        "--source", default="knowledge/documents",
        help="Directory containing documents to ingest"
    )
    parser.add_argument(
        "--persist", default="/zerobot/data/chromadb",
        help="ChromaDB persist directory"
    )
    parser.add_argument(
        "--rebuild", action="store_true",
        help="Clear existing index and re-ingest"
    )
    parser.add_argument(
        "--chunk-size", type=int, default=512,
        help="Document chunk size (characters)"
    )
    parser.add_argument(
        "--chunk-overlap", type=int, default=64,
        help="Chunk overlap (characters)"
    )

    args = parser.parse_args()
    count = ingest(
        source_dir=args.source,
        persist_dir=args.persist,
        chunk_size=args.chunk_size,
        chunk_overlap=args.chunk_overlap,
        rebuild=args.rebuild,
    )
    logger.info("Ingested %d documents.", count)


if __name__ == "__main__":
    main()
