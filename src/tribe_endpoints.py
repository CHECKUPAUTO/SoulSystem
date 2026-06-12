"""
TRIBE Hybrid Endpoints
Enregistre les endpoints améliorés au runtime.
"""

import sys
sys.path.insert(0, '/root')

from tribe_hybrid_search import BM25
from tribe_advanced_chunking import OverlappingChunker, HierarchicalChunker
from tribe_query_parsing import QueryExpansion, QueryRouter
from pydantic import BaseModel

# Variables globales
bm25_index = None
document_corpus = []
expander = None
router = None

# Modèles Pydantic
class ChunkRequest(BaseModel):
    text: str
    method: str = "overlapping"
    chunk_size: int = 512
    overlap: int = 50

class ParseRequest(BaseModel):
    query: str

class HybridSearchRequest(BaseModel):
    query: str
    top_k: int = 10

def init_hybrid_components():
    """Initialise les composants hybrid search."""
    global bm25_index, expander, router
    
    try:
        bm25_index = BM25()
        expander = QueryExpansion()
        router = QueryRouter()
        print("[HYBRID] Composants initialisés: BM25, QueryExpansion, QueryRouter")
        return True
    except Exception as e:
        print(f"[HYBRID] Erreur initialisation: {e}")
        return False

def update_bm25_corpus(documents):
    """Met à jour le corpus BM25."""
    global bm25_index, document_corpus
    document_corpus = documents
    if bm25_index:
        bm25_index.fit(documents)
        print(f"[HYBRID] Corpus BM25 mis à jour: {len(documents)} documents")

def register_hybrid_endpoints(app):
    """
    Enregistre les endpoints hybrid sur l'app FastAPI.
    Les endpoints accèdent au state global via import.
    """
    
    @app.post("/hybrid/search")
    async def hybrid_search(req: HybridSearchRequest):
        """
        Recherche hybride BM25 + Vector Search.
        
        Utilise les améliorations NVIDIA DLI:
        - BM25 pour keyword search
        - Vector search pour semantic search
        - Query expansion pour améliorer la précision
        """
        global expander, bm25_index, document_corpus
        
        try:
            # 1. Query expansion
            expanded = [req.query]
            if expander:
                expanded = expander.expand(req.query, n_expansions=2)
            
            # 2. BM25 search (si corpus disponible)
            bm25_results = []
            if document_corpus and bm25_index:
                try:
                    scores = bm25_index.score(req.query, document_corpus)
                    top_indices = scores.argsort()[::-1][:req.top_k]
                    bm25_results = [
                        {"doc_id": int(idx), "score": float(scores[idx])}
                        for idx in top_indices if scores[idx] > 0
                    ]
                except Exception as e:
                    print(f"[HYBRID] BM25 error: {e}")
            
            # 3. Vector search via import du module
            vector_results = []
            try:
                import tribe_msa_server
                if tribe_msa_server.state and tribe_msa_server.state.tribe:
                    vector_results = await tribe_msa_server.state.tribe.search(req.query, top_k=req.top_k)
            except Exception as e:
                print(f"[HYBRID] Vector search error: {e}")
            
            # 4. Reciprocal Rank Fusion
            combined = {}
            k = 60
            
            for i, r in enumerate(vector_results):
                doc_id = r.get("id", i)
                if doc_id not in combined:
                    combined[doc_id] = 0
                combined[doc_id] += 1.0 / (k + i)
            
            for i, r in enumerate(bm25_results):
                doc_id = r["doc_id"]
                if doc_id not in combined:
                    combined[doc_id] = 0
                combined[doc_id] += 0.3 / (k + i)
            
            # 5. Trier
            sorted_results = sorted(combined.items(), key=lambda x: x[1], reverse=True)
            
            return {
                "status": "ok",
                "query": req.query,
                "expanded_queries": expanded,
                "vector_count": len(vector_results),
                "bm25_count": len(bm25_results),
                "results": [{"id": str(k), "score": float(v)} for k, v in sorted_results[:req.top_k]]
            }
            
        except Exception as e:
            return {"status": "error", "error": str(e)}
    
    @app.post("/hybrid/chunk")
    async def advanced_chunk(req: ChunkRequest):
        """
        Chunking avancé avec méthodes multiples.
        
        Méthodes:
        - overlapping: Fenêtres chevauchantes
        - hierarchical: Structure doc → section → para
        """
        try:
            if req.method == "overlapping":
                chunker = OverlappingChunker(
                    chunk_size=req.chunk_size,
                    overlap=req.overlap
                )
                chunks = chunker.chunk(req.text)
            elif req.method == "hierarchical":
                chunker = HierarchicalChunker()
                result = chunker.chunk(req.text)
                chunks = result.get("paragraphs", [])
            else:
                chunks = [req.text]
            
            return {
                "status": "ok",
                "method": req.method,
                "num_chunks": len(chunks),
                "chunks": chunks[:10]
            }
            
        except Exception as e:
            return {"status": "error", "error": str(e)}
    
    @app.post("/hybrid/parse")
    async def parse_query(req: ParseRequest):
        """
        Parsing de requête avec expansion et routing.
        
        Retourne:
        - expanded_queries: Variations de la requête
        - route: Type de requête
        """
        global expander, router
        
        try:
            expanded = [req.query]
            route = "general"
            
            if expander:
                expanded = expander.expand(req.query, n_expansions=3)
            
            if router:
                route = router.route(req.query)
            
            return {
                "status": "ok",
                "query": req.query,
                "expanded_queries": expanded,
                "route": route
            }
            
        except Exception as e:
            return {"status": "error", "error": str(e)}
    
    print("[HYBRID] Endpoints enregistrés: /hybrid/search, /hybrid/chunk, /hybrid/parse")
    
    return app
