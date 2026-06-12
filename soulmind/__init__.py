"""SoulMind — structured knowledge extraction bridge for SoulSystem.

Patterns borrowed from QuantMind (NeurIPS 2025):
- ``magic.resolve`` — natural language → typed Pydantic model
- ``BaseKnowledge`` / ``TreeKnowledge`` — structured output types
- ``bridge`` — SoulSystem API integration for resolve/extract/store

Quick start::

    from soulmind.magic import resolve
    from soulmind.knowledge import SourceRef

    src = await resolve("arXiv 2401.12345 on momentum", SourceRef)
    print(src.kind, src.uri)  # → "arxiv" "2401.12345"

Server::

    python -m soulmind.server --port 9025
"""

from soulmind.knowledge import (
    BaseKnowledge,
    ExtractionRef,
    FlattenKnowledge,
    GraphKnowledge,
    KnowledgeEdge,
    SourceRef,
    TreeNode,
    TreeKnowledge,
)
from soulmind.magic import resolve, resolve_sync

__all__ = [
    # Knowledge types
    "BaseKnowledge",
    "FlattenKnowledge",
    "TreeKnowledge",
    "TreeNode",
    "GraphKnowledge",
    "KnowledgeEdge",
    "SourceRef",
    "ExtractionRef",
    # Magic
    "resolve",
    "resolve_sync",
]
__version__ = "0.1.0"
