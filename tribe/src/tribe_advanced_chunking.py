"""
Advanced Chunking pour TRIBE
Semantic chunking + Overlapping windows + Hierarchical indexing
"""

import numpy as np
from typing import List, Tuple, Optional
import re

class SemanticChunker:
    """
    Chunking sémantique basé sur la similarité des embeddings.
    Découpe aux limites sémantiques naturelles.
    """
    
    def __init__(self, 
                 embedding_fn,
                 similarity_threshold: float = 0.7,
                 max_chunk_size: int = 512,
                 min_chunk_size: int = 50):
        """
        Args:
            embedding_fn: Fonction pour obtenir les embeddings
            similarity_threshold: Seuil de similarité pour couper
            max_chunk_size: Taille max d'un chunk (tokens)
            min_chunk_size: Taille min d'un chunk (tokens)
        """
        self.embedding_fn = embedding_fn
        self.similarity_threshold = similarity_threshold
        self.max_chunk_size = max_chunk_size
        self.min_chunk_size = min_chunk_size
    
    def chunk(self, text: str) -> List[str]:
        """
        Découpe le texte en chunks sémantiques.
        
        Args:
            text: Texte à découper
        
        Returns:
            Liste de chunks
        """
        # Diviser en phrases
        sentences = self._split_sentences(text)
        
        if len(sentences) == 0:
            return []
        
        # Obtenir les embeddings des phrases
        embeddings = []
        for sent in sentences:
            emb = self.embedding_fn(sent)
            embeddings.append(emb)
        
        embeddings = np.array(embeddings)
        
        # Chunking sémantique
        chunks = []
        current_chunk = [sentences[0]]
        current_start = 0
        
        for i in range(1, len(sentences)):
            # Similarité avec le chunk actuel
            chunk_embedding = np.mean(embeddings[current_start:i], axis=0)
            similarity = self._cosine_similarity(embeddings[i], chunk_embedding)
            
            # Taille actuelle du chunk
            chunk_size = sum(len(s.split()) for s in current_chunk)
            
            # Décider de couper
            if similarity < self.similarity_threshold or chunk_size >= self.max_chunk_size:
                if len(current_chunk) >= self.min_chunk_size:
                    chunks.append(' '.join(current_chunk))
                    current_chunk = [sentences[i]]
                    current_start = i
                else:
                    current_chunk.append(sentences[i])
            else:
                current_chunk.append(sentences[i])
        
        # Dernier chunk
        if current_chunk:
            chunks.append(' '.join(current_chunk))
        
        return chunks
    
    def _split_sentences(self, text: str) -> List[str]:
        """Divise le texte en phrases."""
        # Regex pour split sur . ! ? mais pas sur abréviations
        sentences = re.split(r'(?<!\w\.\w.)(?<![A-Z][a-z]\.)(?<=\.|\?|\!)\s', text)
        return [s.strip() for s in sentences if s.strip()]
    
    def _cosine_similarity(self, a: np.ndarray, b: np.ndarray) -> float:
        """Similarité cosinus entre deux vecteurs."""
        return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-8)


class OverlappingChunker:
    """
    Chunking avec fenêtres chevauchantes.
    Préserve le contexte entre chunks.
    """
    
    def __init__(self, 
                 chunk_size: int = 512,
                 overlap: int = 50):
        """
        Args:
            chunk_size: Taille des chunks (tokens)
            overlap: Nombre de tokens qui se chevauchent
        """
        self.chunk_size = chunk_size
        self.overlap = overlap
    
    def chunk(self, text: str) -> List[str]:
        """
        Découpe avec chevauchement.
        
        Args:
            text: Texte à découper
        
        Returns:
            Liste de chunks avec overlap
        """
        words = text.split()
        
        if len(words) <= self.chunk_size:
            return [text]
        
        chunks = []
        start = 0
        
        while start < len(words):
            end = start + self.chunk_size
            chunk_words = words[start:end]
            chunks.append(' '.join(chunk_words))
            
            # Avancer avec overlap
            start = end - self.overlap
            
            if start >= len(words) - self.overlap:
                break
        
        return chunks


class HierarchicalChunker:
    """
    Chunking hiérarchique: Document → Section → Paragraphe.
    Préserve la structure du document.
    """
    
    def __init__(self, 
                 doc_size: int = 4096,
                 section_size: int = 1024,
                 paragraph_size: int = 256):
        """
        Args:
            doc_size: Taille max document (tokens)
            section_size: Taille max section (tokens)
            paragraph_size: Taille max paragraphe (tokens)
        """
        self.doc_size = doc_size
        self.section_size = section_size
        self.paragraph_size = paragraph_size
    
    def chunk(self, text: str) -> dict:
        """
        Crée une structure hiérarchique.
        
        Returns:
            {
                'document': text,
                'sections': [...],
                'paragraphs': [...]
            }
        """
        # Diviser en sections (par \n\n ou titres)
        sections = re.split(r'\n\n+|\n#{1,3}\s', text)
        
        # Diviser chaque section en paragraphes
        paragraphs = []
        for section in sections:
            paras = re.split(r'\n\n+', section)
            paragraphs.extend(paras)
        
        return {
            'document': text[:self.doc_size],
            'sections': sections,
            'paragraphs': paragraphs,
            'metadata': {
                'num_sections': len(sections),
                'num_paragraphs': len(paragraphs),
                'total_tokens': len(text.split())
            }
        }


# Test
if __name__ == "__main__":
    text = """
    La sauvegarde de base de données est essentielle pour tout système critique.
    Les sauvegardes automatiques doivent être configurées avec une rotation appropriée.
    
    La sécurité des données passe par plusieurs couches de protection.
    Il est important de tester régulièrement les restaurations.
    
    Les performances du serveur dépendent de nombreux facteurs.
    L'optimisation du cache et des index peut améliorer significativement les temps de réponse.
    """
    
    # Overlapping chunker
    chunker = OverlappingChunker(chunk_size=50, overlap=10)
    chunks = chunker.chunk(text)
    
    print(f"Chunks créés: {len(chunks)}")
    for i, chunk in enumerate(chunks):
        print(f"\nChunk {i+1}:")
        print(f"  {chunk[:100]}...")
    
    print("\n✓ Advanced Chunking prêt!")
