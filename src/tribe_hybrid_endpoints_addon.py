"""
TRIBE Hybrid Endpoints - Addon
À ajouter à la fin du fichier tribe_msa_server.py
"""

# === HYBRID SEARCH ENDPOINT ===
@app.post("/hybrid/search")
async def hybrid_search_endpoint(
    query: str,
    top_k: int = 10
):
    """Recherche hybride BM25 + Vector Search."""
    global expander, bm25_index, document_corpus
    
    try:
        from tribe_query_parsing import QueryExpansion
        
        # Query expansion
        expanded = [query]
        if expander is None:
            expander = QueryExpansion()
        expanded = expander.expand(query, n_expansions=2)
        
        # Vector search via state
        vector_results = []
        if state and state.tribe:
            vector_results = await state.tribe.search(query, top_k=top_k)
        
        # BM25 search (si corpus)
        bm25_results = []
        # TODO: ajouter corpus
        
        return {
            "status": "ok",
            "query": query,
            "expanded_queries": expanded,
            "vector_count": len(vector_results),
            "results": vector_results[:top_k]
        }
    except Exception as e:
        return {"status": "error", "error": str(e)}


# === ADVANCED CHUNKING ENDPOINT ===
@app.post("/hybrid/chunk")
async def advanced_chunk_endpoint(
    text: str,
    method: str = "overlapping",
    chunk_size: int = 512,
    overlap: int = 50
):
    """Chunking avancé."""
    try:
        from tribe_advanced_chunking import OverlappingChunker, HierarchicalChunker
        
        if method == "overlapping":
            chunker = OverlappingChunker(chunk_size=chunk_size, overlap=overlap)
            chunks = chunker.chunk(text)
        elif method == "hierarchical":
            chunker = HierarchicalChunker()
            result = chunker.chunk(text)
            chunks = result.get("paragraphs", [])
        else:
            chunks = [text]
        
        return {
            "status": "ok",
            "method": method,
            "num_chunks": len(chunks),
            "chunks": chunks[:10]
        }
    except Exception as e:
        return {"status": "error", "error": str(e)}


# === QUERY PARSING ENDPOINT ===
@app.post("/hybrid/parse")
async def parse_query_endpoint(query: str):
    """Parsing de requête avec expansion et routing."""
    try:
        from tribe_query_parsing import QueryExpansion, QueryRouter
        
        expander = QueryExpansion()
        router = QueryRouter()
        
        expanded = expander.expand(query, n_expansions=3)
        route = router.route(query)
        
        return {
            "status": "ok",
            "query": query,
            "expanded_queries": expanded,
            "route": route
        }
    except Exception as e:
        return {"status": "error", "error": str(e)}
