"""Shared knowledge types for SoulMind — adapted from QuantMind patterns.

Three shapes:
- ``FlattenKnowledge`` — atomic items (extracted facts, summaries)
- ``TreeKnowledge`` — hierarchical structured output (papers, reports)
- ``GraphKnowledge`` — cross-item edges (placeholder)

Every item carries ``as_of``, ``confidence``, and typed ``SourceRef``
so provenance and freshness are explicit.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, ClassVar, Literal
from uuid import UUID, uuid4

from pydantic import BaseModel, ConfigDict, Field


# ── Provenance primitives ───────────────────────────────────────────

class SourceRef(BaseModel):
    """Typed provenance. Replaces bare ``source: str``."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    kind: Literal[
        "arxiv", "http", "doi", "local", "telegram",
        "internal", "zerobot", "forge", "manual",
    ]
    uri: str | None = None
    fetched_at: datetime | None = None
    content_hash: str | None = None


class ExtractionRef(BaseModel):
    """Which flow + model produced this item."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    flow: str
    model: str
    run_id: UUID | None = None
    extracted_at: datetime = Field(
        default_factory=lambda: datetime.now(timezone.utc)
    )


# ── Base knowledge ──────────────────────────────────────────────────

class BaseKnowledge(BaseModel):
    """Root for every SoulMind knowledge type."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    id: UUID = Field(default_factory=uuid4)
    item_type: str
    schema_version: str = "1.0"

    # Time — financial/scientific mandate
    as_of: datetime = Field(
        default_factory=lambda: datetime.now(timezone.utc),
        description="Information cutoff time.",
    )
    created_at: datetime = Field(
        default_factory=lambda: datetime.now(timezone.utc),
    )

    # Provenance
    source: SourceRef
    extraction: ExtractionRef | None = None

    # Trust
    confidence: Literal["low", "medium", "high"] = "medium"

    # Metadata
    tags: list[str] = Field(default_factory=list)
    summary: str = ""

    def embedding_text(self) -> str:
        """Canonical string to embed. Subclasses override."""
        return self.summary

    def is_extracted(self) -> bool:
        return self.extraction is not None


# ── Flatten knowledge ───────────────────────────────────────────────

class FlattenKnowledge(BaseKnowledge):
    """An atomic extracted fact or item.

    Subclass and add fields for your domain (news, factor, earnings, etc.).
    """

    item_type: str = "flatten"
    payload: dict[str, Any] = Field(default_factory=dict)


# ── Tree knowledge ──────────────────────────────────────────────────

class TreeNode(BaseModel):
    """A single node in a ``TreeKnowledge`` hierarchy."""

    model_config = ConfigDict(extra="forbid")

    id: UUID = Field(default_factory=uuid4)
    title: str
    summary: str = ""
    content: str = ""  # Markdown content for leaf nodes
    children: list[TreeNode] = Field(default_factory=list)
    citations: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)

    def is_leaf(self) -> bool:
        return len(self.children) == 0

    def depth(self, current: int = 0) -> int:
        if not self.children:
            return current
        return max(c.depth(current + 1) for c in self.children)


class TreeKnowledge(BaseKnowledge):
    """Hierarchical extracted knowledge (paper, report, analysis)."""

    item_type: str = "tree"
    root: TreeNode

    def embedding_text(self) -> str:
        return self.root.title + ". " + self.root.summary

    def flatten(self) -> list[TreeNode]:
        """DFS flatten of the tree."""
        nodes: list[TreeNode] = []
        stack = [self.root]
        while stack:
            node = stack.pop()
            nodes.append(node)
            stack.extend(reversed(node.children))
        return nodes


# ── Graph knowledge ─────────────────────────────────────────────────

class KnowledgeEdge(BaseModel):
    """A directed edge between two knowledge items."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    source_id: UUID
    target_id: UUID
    relation: str  # "cites", "contradicts", "extends", "summarizes"
    weight: float = 1.0
    evidence: str = ""


class GraphKnowledge(BaseKnowledge):
    """Cross-item knowledge graph (placeholder for future PR)."""

    item_type: str = "graph"
    nodes: list[UUID] = Field(default_factory=list)
    edges: list[KnowledgeEdge] = Field(default_factory=list)
