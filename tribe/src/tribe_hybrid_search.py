"""
Hybrid Search pour TRIBE
Combinaison BM25 + Vector Search + Reciprocal Rank Fusion
"""

import numpy as np
from typing import List, Dict, Tuple, Optional
import re
from collections import Counter
import math

class BM25:
    """
    BM25 (Best Match 25) pour recherche keyword.
    Plus performant que TF-IDF pour la recherche textuelle.
    """
    
    def __init__(self, k1: float = 1.5, b: float = 0.75):
        self.k1 = k1
        self.b = b
        self.doc_lengths = []
        self.avg_doc_length = 0
        self.doc_count = 0
        self.term_doc_freq = Counter()
        self.doc_terms = []
    
    def fit(self, documents: List[str]):
        """Calcule les statistiques BM25."""
        self.doc_count = len(documents)
        self.doc_lengths = [len(doc.split()) for doc in documents]
        self.avg_doc_length = np.mean(self.doc_lengths) if self.doc_lengths else 1
        
        # Compter les termes par document
        self.doc_terms = []
        for doc in documents:
            terms = self._tokenize(doc)
            self.doc_terms.append(Counter(terms))
            self.term_doc_freq.update(set(terms))
    
    def _tokenize(self, text: str) -> List[str]:
        """Tokenisation simple."""
        text = text.lower()
        text = re.sub(r'[^\w\s]', ' ', text)
        return text.split()
    
    def score(self, query: str, documents: List[str]) -> np.ndarray:
        """Calcule les scores BM25 pour une requête."""
        if not self.doc_terms:
            return np.zeros(len(documents))
        
        query_terms = self._tokenize(query)
        scores = np.zeros(len(documents))
        
        for term in query_terms:
            if term not in self.term_doc_freq:
                continue
            
            # IDF (Inverse Document Frequency)
            df = self.term_doc_freq[term]
            idf = math.log((self.doc_count - df + 0.5) / (df + 0.5) + 1)
            
            # Score pour chaque document
            for i, doc_counter in enumerate(self.doc_terms):
                tf = doc_counter.get(term, 0)
                doc_len = self.doc_lengths[i]
                
                # BM25 formula
                numerator = tf * (self.k1 + 1)
                denominator = tf + self.k1 * (1 - self.b + self.b * doc_len / self.avg_doc_length)
                scores[i] += idf * numerator / denominator
        
        return scores


class HybridSearch:
    """
    Hybrid Search combinant BM25 et Vector Search.
    Utilise Reciprocal Rank Fusion (RRF) pour combiner les résultats.
    """
    
    def __init__(self, 
                 vector_search_fn,
                 documents: List[str],
                 alpha: float = 0.5,
                 k: int = 60):
        """
        Args:
            vector_search_fn: Fonction de recherche vectorielle
            documents: Liste des documents
            alpha: Poids BM25 (1-alpha = poids vector)
            k: Paramètre RRF
        """
        self.vector_search_fn = vector_search_fn
        self.documents = documents
        self.alpha = alpha
        self.k = k
        self.bm25 = BM25()
        self.bm25.fit(documents)
    
    def search(self, query: str, top_k: int = 10) -> List[Tuple[int, float]]:
        """
        Recherche hybride BM25 + Vector.
        
        Args:
            query: Requête texte
            top_k: Nombre de résultats
        
        Returns:
            Liste de (doc_id, score)
        """
        # BM25 scores
        bm25_scores = self.bm25.score(query, self.documents)
        
        # Vector search scores (normalisés)
        vector_scores = self.vector_search_fn(query, top_k=len(self.documents))
        
        # Normaliser les scores
        bm25_norm = self._normalize(bm25_scores)
        vector_norm = self._normalize(vector_scores)
        
        # Combiner avec RRF
        combined_scores = self._reciprocal_rank_fusion(bm25_norm, vector_norm)
        
        # Trier et retourner top_k
        ranked_indices = np.argsort(combined_scores)[::-1][:top_k]
        
        return [(idx, combined_scores[idx]) for idx in ranked_indices]
    
    def _normalize(self, scores: np.ndarray) -> np.ndarray:
        """Normalise les scores entre 0 et 1."""
        if scores.max() == scores.min():
            return np.zeros_like(scores)
        return (scores - scores.min()) / (scores.max() - scores.min())
    
    def _reciprocal_rank_fusion(self, 
                                bm25_scores: np.ndarray, 
                                vector_scores: np.ndarray) -> np.ndarray:
        """
        Reciprocal Rank Fusion (RRF).
        
        RRF(d) = Σ 1/(k + rank(d))
        """
        # Calculer les rangs BM25
        bm25_ranks = np.argsort(np.argsort(bm25_scores)[::-1])
        
        # Calculer les rangs vector
        vector_ranks = np.argsort(np.argsort(vector_scores)[::-1])
        
        # RRF score
        rrf_scores = self.alpha / (self.k + bm25_ranks) + \
                     (1 - self.alpha) / (self.k + vector_ranks)
        
        return rrf_scores


# Test
if __name__ == "__main__":
    # Documents de test
    documents = [
        "sauvegarder base de données",
        "backup database automatique",
        "redémarrer service apache",
        "optimiser performances serveur",
        "analyser logs système",
        "configurer nginx reverse proxy",
        "sécuriser serveur linux",
        "installer docker container",
    ]
    
    # BM25 simple
    bm25 = BM25()
    bm25.fit(documents)
    
    query = "backup database"
    scores = bm25.score(query, documents)
    
    print("BM25 Scores:")
    for i, (doc, score) in enumerate(zip(documents, scores)):
        print(f"  {i}: {score:.3f} - {doc}")
    
    print("\n✓ Hybrid Search prêt!")
