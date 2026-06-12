"""
Intégration complète des améliorations TRIBE
Hybrid Search + Advanced Chunking + Query Parsing
"""

import sys
sys.path.insert(0, '/root')

from tribe_hybrid_search import HybridSearch, BM25
from tribe_advanced_chunking import OverlappingChunker, HierarchicalChunker
from tribe_query_parsing import QueryExpansion, QueryRouter

class TRIBEEnhancements:
    """
    Wrapper pour toutes les améliorations TRIBE.
    """
    
    def __init__(self, vector_search_fn=None, documents=None):
        """
        Initialise les améliorations.
        
        Args:
            vector_search_fn: Fonction de recherche vectorielle
            documents: Liste des documents pour BM25
        """
        self.vector_search_fn = vector_search_fn
        self.documents = documents or []
        
        # Composants
        self.bm25 = BM25()
        self.hybrid_search = None
        self.overlapping_chunker = OverlappingChunker(chunk_size=512, overlap=50)
        self.hierarchical_chunker = HierarchicalChunker()
        self.query_expander = QueryExpansion()
        self.query_router = QueryRouter()
        
        # Initialiser BM25 si documents fournis
        if documents:
            self.bm25.fit(documents)
            self.hybrid_search = HybridSearch(
                vector_search_fn=vector_search_fn,
                documents=documents,
                alpha=0.3  # 30% BM25, 70% vector
            )
    
    def update_documents(self, documents):
        """Met à jour les documents pour BM25."""
        self.documents = documents
        self.bm25.fit(documents)
        
        if self.vector_search_fn:
            self.hybrid_search = HybridSearch(
                vector_search_fn=self.vector_search_fn,
                documents=documents,
                alpha=0.3
            )
    
    def chunk_document(self, text: str, method: str = 'overlapping') -> list:
        """
        Découpe un document avec la méthode spécifiée.
        
        Args:
            text: Texte à découper
            method: 'overlapping' ou 'hierarchical'
        
        Returns:
            Liste de chunks ou structure hiérarchique
        """
        if method == 'overlapping':
            return self.overlapping_chunker.chunk(text)
        elif method == 'hierarchical':
            return self.hierarchical_chunker.chunk(text)
        else:
            return [text]
    
    def expand_query(self, query: str, n: int = 3) -> list:
        """Génère des variations de la requête."""
        return self.query_expander.expand(query, n_expansions=n)
    
    def route_query(self, query: str) -> str:
        """Route la requête vers le bon index."""
        return self.query_router.route(query)
    
    def hybrid_search_query(self, query: str, top_k: int = 10) -> list:
        """
        Recherche hybride BM25 + Vector.
        
        Args:
            query: Requête texte
            top_k: Nombre de résultats
        
        Returns:
            Liste de (doc_id, score)
        """
        if self.hybrid_search is None:
            # Fallback vers BM25 seul
            scores = self.bm25.score(query, self.documents)
            ranked_indices = scores.argsort()[::-1][:top_k]
            return [(idx, scores[idx]) for idx in ranked_indices]
        
        return self.hybrid_search.search(query, top_k=top_k)


# Fonction d'intégration pour TRIBE
def create_enhancements(vector_search_fn=None, documents=None):
    """Crée une instance des améliorations."""
    return TRIBEEnhancements(vector_search_fn, documents)


print("✓ TRIBE Integration prête!")
