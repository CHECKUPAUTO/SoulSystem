"""
Intégration des améliorations TRIBE
Hybrid Search + Advanced Chunking + Query Parsing
"""

from tribe_hybrid_search import HybridSearch, BM25
from tribe_advanced_chunking import SemanticChunker, OverlappingChunker, HierarchicalChunker
from tribe_query_parsing import QueryExpansion, MultiQueryRetrieval, QueryRouter

__all__ = [
    'HybridSearch',
    'BM25',
    'SemanticChunker',
    'OverlappingChunker',
    'HierarchicalChunker',
    'QueryExpansion',
    'MultiQueryRetrieval',
    'QueryRouter',
]

print("✓ TRIBE Enhancements prêts!")
