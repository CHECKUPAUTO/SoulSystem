"""
Query Parsing pour TRIBE
Query expansion + Multi-query retrieval + Query routing
"""

import re
from typing import List, Dict, Tuple, Optional
import numpy as np

class QueryExpansion:
    """
    Expansion de requête avec synonymes et termes connexes.
    """
    
    def __init__(self):
        # Dictionnaire de synonymes techniques
        self.synonyms = {
            'sauvegarder': ['backup', 'sauvegarde', 'enregistrer', 'copier'],
            'backup': ['sauvegarder', 'sauvegarde', 'copie'],
            'base de données': ['database', 'db', 'bdd', 'base'],
            'database': ['base de données', 'bdd', 'base'],
            'redémarrer': ['restart', 'reboot', 'relancer'],
            'restart': ['redémarrer', 'reboot', 'relancer'],
            'optimiser': ['optimization', 'améliorer', 'performance'],
            'analyser': ['analyse', 'examiner', 'vérifier'],
            'logs': ['log', 'journal', 'trace'],
            'sécuriser': ['sécurité', 'security', 'protection'],
            'configurer': ['configuration', 'config', 'setup'],
            'installer': ['installation', 'install', 'setup'],
        }
    
    def expand(self, query: str, n_expansions: int = 3) -> List[str]:
        """
        Génère des requêtes expansées.
        
        Args:
            query: Requête originale
            n_expansions: Nombre de variations
        
        Returns:
            Liste de requêtes expansées
        """
        # Tokeniser
        words = query.lower().split()
        
        # Trouver les synonymes
        expansions = [query]
        
        for word in words:
            if word in self.synonyms:
                for syn in self.synonyms[word][:n_expansions]:
                    expanded = query.lower().replace(word, syn)
                    if expanded not in expansions:
                        expansions.append(expanded)
        
        return expansions[:n_expansions + 1]


class MultiQueryRetrieval:
    """
    Multi-query retrieval: génère plusieurs variantes et fusionne les résultats.
    """
    
    def __init__(self, search_fn, n_queries: int = 3):
        """
        Args:
            search_fn: Fonction de recherche
            n_queries: Nombre de variantes à générer
        """
        self.search_fn = search_fn
        self.n_queries = n_queries
        self.expander = QueryExpansion()
    
    def search(self, query: str, top_k: int = 10) -> List[Tuple[int, float]]:
        """
        Recherche multi-query.
        
        Args:
            query: Requête originale
            top_k: Nombre de résultats
        
        Returns:
            Liste fusionnée de (doc_id, score)
        """
        # Générer les variantes
        queries = self.expander.expand(query, n_expansions=self.n_queries)
        
        # Collecter les résultats
        all_results = {}
        
        for q in queries:
            results = self.search_fn(q, top_k=top_k * 2)
            
            for doc_id, score in results:
                if doc_id in all_results:
                    all_results[doc_id] = max(all_results[doc_id], score)
                else:
                    all_results[doc_id] = score
        
        # Trier par score
        sorted_results = sorted(all_results.items(), key=lambda x: x[1], reverse=True)
        
        return sorted_results[:top_k]


class QueryRouter:
    """
    Route les requêtes vers le bon index selon le type de question.
    """
    
    def __init__(self):
        # Patterns pour router
        self.patterns = {
            'technical': [
                r'comment\s+\w+',
                r'pourquoi\s+\w+',
                r'expliquer\s+\w+',
                r'différence\s+entre',
            ],
            'action': [
                r'sauvegarder\s+\w+',
                r'redémarrer\s+\w+',
                r'installer\s+\w+',
                r'configurer\s+\w+',
                r'optimiser\s+\w+',
                r'analyser\s+\w+',
            ],
            'error': [
                r'erreur\s+\w+',
                r'problème\s+\w+',
                r'ne\s+fonctionne\s+pas',
                r'échec\s+\w+',
            ],
            'search': [
                r'trouver\s+\w+',
                r'chercher\s+\w+',
                r'où\s+est\s+\w+',
            ]
        }
    
    def route(self, query: str) -> str:
        """
        Route la requête vers le bon index.
        
        Returns:
            Type de requête: 'technical', 'action', 'error', 'search', 'general'
        """
        query_lower = query.lower()
        
        for query_type, patterns in self.patterns.items():
            for pattern in patterns:
                if re.search(pattern, query_lower):
                    return query_type
        
        return 'general'


# Test
if __name__ == "__main__":
    # Query Expansion
    expander = QueryExpansion()
    
    query = "sauvegarder base de données"
    expansions = expander.expand(query, n_expansions=3)
    
    print(f"Requête: {query}")
    print(f"Expansions:")
    for i, exp in enumerate(expansions, 1):
        print(f"  {i}. {exp}")
    
    # Query Router
    router = QueryRouter()
    
    queries = [
        "comment sauvegarder une base de données",
        "redémarrer le serveur apache",
        "erreur connexion base de données",
        "trouver les logs système",
    ]
    
    print("\nRouting:")
    for q in queries:
        route = router.route(q)
        print(f"  '{q}' -> {route}")
    
    print("\n✓ Query Parsing prêt!")
