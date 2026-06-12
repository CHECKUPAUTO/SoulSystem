"""
TRIBE RAPIDS Data Science
Accélération GPU pour le traitement de données avec RAPIDS

Utilise cuDF, cuML et cuGraph pour le traitement de données accéléré GPU.
"""

import numpy as np
from typing import List, Dict, Any, Optional, Tuple
import os

# Configuration
RAPIDS_AVAILABLE = False

try:
    import cudf
    import cuml
    import cugraph
    RAPIDS_AVAILABLE = True
    print("[RAPIDS] cuDF, cuML, cuGraph disponibles")
except ImportError:
    print("[RAPIDS] RAPIDS non installé, fallback vers pandas/sklearn")


class RAPIDSDataProcessor:
    """
    Processeur de données accéléré GPU avec RAPIDS.
    +50-100x plus rapide que pandas/sklearn sur GPU.
    """
    
    def __init__(self, use_gpu: bool = True):
        """
        Initialise le processeur RAPIDS.
        
        Args:
            use_gpu: Utiliser le GPU si disponible
        """
        self.use_gpu = use_gpu and RAPIDS_AVAILABLE
        self.df = None
        
        if self.use_gpu:
            print("[RAPIDS] Mode GPU activé")
        else:
            print("[RAPIDS] Mode CPU (fallback)")
    
    def load_data(self, data: Any, format: str = "auto") -> Any:
        """
        Charge des données en mémoire GPU.
        
        Args:
            data: Données à charger
            format: Format des données (auto, csv, json, parquet)
        
        Returns:
            DataFrame GPU ou CPU
        """
        if self.use_gpu:
            if isinstance(data, str):
                if data.endswith('.csv'):
                    self.df = cudf.read_csv(data)
                elif data.endswith('.parquet'):
                    self.df = cudf.read_parquet(data)
                elif data.endswith('.json'):
                    self.df = cudf.read_json(data)
                else:
                    self.df = cudf.DataFrame(data)
            else:
                self.df = cudf.DataFrame(data)
        else:
            import pandas as pd
            if isinstance(data, str):
                self.df = pd.read_csv(data) if data.endswith('.csv') else pd.DataFrame(data)
            else:
                self.df = pd.DataFrame(data)
        
        return self.df
    
    def process_embeddings(self, embeddings: np.ndarray) -> np.ndarray:
        """
        Traite des embeddings en masse sur GPU.
        
        Args:
            embeddings: Array d'embeddings
        
        Returns:
            Embeddings traités
        """
        if self.use_gpu:
            import cupy as cp
            # Transférer sur GPU
            gpu_embeddings = cp.array(embeddings)
            
            # Normalisation sur GPU
            norms = cp.linalg.norm(gpu_embeddings, axis=1, keepdims=True)
            normalized = gpu_embeddings / (norms + 1e-8)
            
            return cp.asnumpy(normalized)
        else:
            # Fallback CPU
            norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
            return embeddings / (norms + 1e-8)
    
    def cluster_embeddings(self, embeddings: np.ndarray, n_clusters: int = 10) -> np.ndarray:
        """
        Clustering d'embeddings avec KMeans GPU.
        
        Args:
            embeddings: Array d'embeddings
            n_clusters: Nombre de clusters
        
        Returns:
            Labels des clusters
        """
        if self.use_gpu:
            import cupy as cp
            from cuml.cluster import KMeans
            
            # Transférer sur GPU
            gpu_embeddings = cp.array(embeddings)
            
            # KMeans sur GPU
            kmeans = KMeans(n_clusters=n_clusters)
            labels = kmeans.fit_predict(gpu_embeddings)
            
            return cp.asnumpy(labels)
        else:
            from sklearn.cluster import KMeans
            kmeans = KMeans(n_clusters=n_clusters)
            return kmeans.fit_predict(embeddings)
    
    def reduce_dimensions(self, embeddings: np.ndarray, n_components: int = 2) -> np.ndarray:
        """
        Réduction de dimension avec UMAP GPU.
        
        Args:
            embeddings: Array d'embeddings
            n_components: Nombre de dimensions cibles
        
        Returns:
            Embeddings réduits
        """
        if self.use_gpu:
            import cupy as cp
            from cuml.manifold import UMAP
            
            # Transférer sur GPU
            gpu_embeddings = cp.array(embeddings)
            
            # UMAP sur GPU
            umap = UMAP(n_components=n_components)
            reduced = umap.fit_transform(gpu_embeddings)
            
            return cp.asnumpy(reduced)
        else:
            from umap import UMAP
            umap = UMAP(n_components=n_components)
            return umap.fit_transform(embeddings)
    
    def search_similar(self, query_embedding: np.ndarray, 
                       corpus_embeddings: np.ndarray, 
                       top_k: int = 10) -> List[Tuple[int, float]]:
        """
        Recherche de similarité accélérée GPU.
        
        Args:
            query_embedding: Embedding de la requête
            corpus_embeddings: Embeddings du corpus
            top_k: Nombre de résultats
        
        Returns:
            Liste de (index, score)
        """
        if self.use_gpu:
            import cupy as cp
            
            # Transférer sur GPU
            gpu_query = cp.array(query_embedding)
            gpu_corpus = cp.array(corpus_embeddings)
            
            # Similarité cosinus sur GPU
            norms_query = cp.linalg.norm(gpu_query)
            norms_corpus = cp.linalg.norm(gpu_corpus, axis=1)
            
            # Produit scalaire
            similarities = cp.dot(gpu_corpus, gpu_query) / (norms_corpus * norms_query + 1e-8)
            
            # Top-k
            top_indices = cp.argsort(similarities)[::-1][:top_k]
            top_scores = similarities[top_indices]
            
            return [(int(idx), float(score)) for idx, score in zip(top_indices, top_scores)]
        else:
            # Fallback CPU
            from sklearn.metrics.pairwise import cosine_similarity
            
            similarities = cosine_similarity([query_embedding], corpus_embeddings)[0]
            top_indices = np.argsort(similarities)[::-1][:top_k]
            
            return [(int(idx), float(similarities[idx])) for idx in top_indices]


class RAPIDSFeatureStore:
    """
    Stockage de features accéléré GPU avec RAPIDS.
    """
    
    def __init__(self, use_gpu: bool = True):
        """Initialise le stockage."""
        self.processor = RAPIDSDataProcessor(use_gpu=use_gpu)
        self.features = None
        self.metadata = None
    
    def store_embeddings(self, embeddings: Dict[str, np.ndarray]):
        """Stocke des embeddings."""
        if self.processor.use_gpu:
            import cudf
            
            # Créer un DataFrame GPU
            ids = list(embeddings.keys())
            vectors = np.array(list(embeddings.values()))
            
            self.features = cudf.DataFrame({
                'id': ids,
                'embedding': list(vectors)
            })
        else:
            import pandas as pd
            
            ids = list(embeddings.keys())
            vectors = np.array(list(embeddings.values()))
            
            self.features = pd.DataFrame({
                'id': ids,
                'embedding': list(vectors)
            })
    
    def retrieve_similar(self, query: np.ndarray, top_k: int = 10) -> List[Tuple[str, float]]:
        """Récupère les embeddings similaires."""
        if self.features is None:
            return []
        
        embeddings = np.array(list(self.features['embedding']))
        ids = list(self.features['id'])
        
        similar = self.processor.search_similar(query, embeddings, top_k)
        
        return [(ids[idx], score) for idx, score in similar]


# Test
if __name__ == "__main__":
    print("RAPIDS Data Science Module for TRIBE")
    print(f"RAPIDS available: {RAPIDS_AVAILABLE}")
    
    # Créer des embeddings de test
    test_embeddings = np.random.randn(1000, 1024).astype(np.float32)
    
    processor = RAPIDSDataProcessor()
    
    # Benchmark
    import time
    
    start = time.time()
    normalized = processor.process_embeddings(test_embeddings)
    elapsed = time.time() - start
    
    print(f"Normalisation de {len(test_embeddings)} embeddings: {elapsed*1000:.2f}ms")
