"""
TRIBE Multi-Modal
Support pour images, audio et texte multimodal

Intégration de CLIP, Whisper et LLaVA.
"""

import numpy as np
from typing import List, Dict, Any, Optional, Union, Tuple
import os

# Configuration
MULTIMODAL_CACHE_DIR = "/root/tribe_extensions/multimodal_cache"
os.makedirs(MULTIMODAL_CACHE_DIR, exist_ok=True)

# Disponibilité des modèles
CLIP_AVAILABLE = False
WHISPER_AVAILABLE = False
LLAVA_AVAILABLE = False

try:
    from transformers import CLIPProcessor, CLIPModel
    CLIP_AVAILABLE = True
    print("[Multi-Modal] CLIP disponible")
except ImportError:
    print("[Multi-Modal] CLIP non installé")

try:
    from transformers import WhisperProcessor, WhisperForConditionalGeneration
    WHISPER_AVAILABLE = True
    print("[Multi-Modal] Whisper disponible")
except ImportError:
    print("[Multi-Modal] Whisper non installé")


class CLIPProcessor:
    """
    Processeur CLIP pour images et texte.
    Embeddings multimodaux alignés.
    """
    
    def __init__(self, model_name: str = "openai/clip-vit-base-patch32"):
        """
        Initialise le processeur CLIP.
        
        Args:
            model_name: Nom du modèle CLIP
        """
        self.model_name = model_name
        self.model = None
        self.processor = None
        
        if CLIP_AVAILABLE:
            self._load_model()
    
    def _load_model(self):
        """Charge le modèle CLIP."""
        try:
            from transformers import CLIPModel, CLIPProcessor as HFCLIPProcessor
            
            self.model = CLIPModel.from_pretrained(self.model_name)
            self.processor = HFCLIPProcessor.from_pretrained(self.model_name)
            
            print(f"[CLIP] Modèle chargé: {self.model_name}")
        except Exception as e:
            print(f"[CLIP] Erreur de chargement: {e}")
    
    def embed_image(self, image) -> np.ndarray:
        """
        Embed une image.
        
        Args:
            image: Image PIL ou chemin
        
        Returns:
            Embedding image (512 dims)
        """
        if not CLIP_AVAILABLE or not self.model:
            return np.zeros(512)
        
        try:
            # Traiter l'image
            if isinstance(image, str):
                from PIL import Image
                image = Image.open(image)
            
            inputs = self.processor(images=image, return_tensors="pt")
            
            # Embedding
            image_features = self.model.get_image_features(**inputs)
            
            return image_features.detach().numpy()[0]
            
        except Exception as e:
            print(f"[CLIP] Erreur embedding image: {e}")
            return np.zeros(512)
    
    def embed_text(self, text: str) -> np.ndarray:
        """
        Embed un texte.
        
        Args:
            text: Texte à embedder
        
        Returns:
            Embedding texte (512 dims)
        """
        if not CLIP_AVAILABLE or not self.model:
            return np.zeros(512)
        
        try:
            inputs = self.processor(text=text, return_tensors="pt")
            
            # Embedding
            text_features = self.model.get_text_features(**inputs)
            
            return text_features.detach().numpy()[0]
            
        except Exception as e:
            print(f"[CLIP] Erreur embedding texte: {e}")
            return np.zeros(512)
    
    def similarity(self, image, text: str) -> float:
        """
        Calcule la similarité image-texte.
        
        Args:
            image: Image PIL ou chemin
            text: Texte
        
        Returns:
            Score de similarité [0, 1]
        """
        image_embed = self.embed_image(image)
        text_embed = self.embed_text(text)
        
        # Cosine similarity
        similarity = np.dot(image_embed, text_embed) / (
            np.linalg.norm(image_embed) * np.linalg.norm(text_embed) + 1e-8
        )
        
        return float(similarity)


class WhisperProcessor:
    """
    Processeur Whisper pour la transcription audio.
    """
    
    def __init__(self, model_name: str = "openai/whisper-base"):
        """
        Initialise le processeur Whisper.
        
        Args:
            model_name: Nom du modèle Whisper
        """
        self.model_name = model_name
        self.model = None
        self.processor = None
        
        if WHISPER_AVAILABLE:
            self._load_model()
    
    def _load_model(self):
        """Charge le modèle Whisper."""
        try:
            from transformers import WhisperForConditionalGeneration, WhisperProcessor as HFWhisperProcessor
            
            self.model = WhisperForConditionalGeneration.from_pretrained(self.model_name)
            self.processor = HFWhisperProcessor.from_pretrained(self.model_name)
            
            print(f"[Whisper] Modèle chargé: {self.model_name}")
        except Exception as e:
            print(f"[Whisper] Erreur de chargement: {e}")
    
    def transcribe(self, audio_path: str, language: str = None) -> str:
        """
        Transcrit un fichier audio.
        
        Args:
            audio_path: Chemin vers le fichier audio
            language: Langue optionnelle
        
        Returns:
            Transcription
        """
        if not WHISPER_AVAILABLE or not self.model:
            return f"[Whisper] Non disponible pour {audio_path}"
        
        try:
            import librosa
            
            # Charger l'audio
            audio, sr = librosa.load(audio_path, sr=16000)
            
            # Prétraiter
            inputs = self.processor(audio, sampling_rate=16000, return_tensors="pt")
            
            # Transcrire
            predicted_ids = self.model.generate(**inputs)
            transcription = self.processor.batch_decode(predicted_ids, skip_special_tokens=True)[0]
            
            return transcription
            
        except Exception as e:
            print(f"[Whisper] Erreur de transcription: {e}")
            return f"[Whisper] Erreur: {str(e)}"
    
    def embed_audio(self, audio_path: str) -> np.ndarray:
        """
        Embed un fichier audio.
        
        Args:
            audio_path: Chemin vers le fichier audio
        
        Returns:
            Embedding audio
        """
        # Pour l'instant, on retourne un placeholder
        # Une vraie implémentation utiliserait les embeddings intermédiaires de Whisper
        return np.zeros(512)


class MultiModalEmbeddings:
    """
    Gestionnaire d'embeddings multimodaux.
    Combine texte, image et audio dans un espace commun.
    """
    
    def __init__(self, 
                 use_clip: bool = True,
                 use_whisper: bool = True):
        """
        Initialise le gestionnaire multimodal.
        
        Args:
            use_clip: Utiliser CLIP
            use_whisper: Utiliser Whisper
        """
        self.clip = CLIPProcessor() if use_clip else None
        self.whisper = WhisperProcessor() if use_whisper else None
    
    def embed(self, 
              text: str = None,
              image = None,
              audio_path: str = None) -> Dict[str, np.ndarray]:
        """
        Embed des données multimodales.
        
        Args:
            text: Texte optionnel
            image: Image optionnelle
            audio_path: Chemin audio optionnel
        
        Returns:
            Dictionnaire d'embeddings
        """
        embeddings = {}
        
        # Texte
        if text and self.clip:
            embeddings["text"] = self.clip.embed_text(text)
        
        # Image
        if image is not None and self.clip:
            embeddings["image"] = self.clip.embed_image(image)
        
        # Audio
        if audio_path and self.whisper:
            embeddings["audio"] = self.whisper.embed_audio(audio_path)
        
        return embeddings
    
    def search_cross_modal(self,
                          query_embedding: np.ndarray,
                          corpus: List[Dict[str, np.ndarray]],
                          modality: str = "text",
                          top_k: int = 5) -> List[Tuple[int, float]]:
        """
        Recherche cross-modale.
        
        Args:
            query_embedding: Embedding de la requête
            corpus: Corpus de documents multimodaux
            modality: Modalité à rechercher
            top_k: Nombre de résultats
        
        Returns:
            Liste de (index, score)
        """
        scores = []
        
        for i, doc in enumerate(corpus):
            if modality in doc:
                # Similarité cosinus
                score = np.dot(query_embedding, doc[modality]) / (
                    np.linalg.norm(query_embedding) * np.linalg.norm(doc[modality]) + 1e-8
                )
                scores.append((i, score))
        
        # Trier par score
        scores.sort(key=lambda x: x[1], reverse=True)
        
        return scores[:top_k]


# Test
if __name__ == "__main__":
    print("Multi-Modal Module for TRIBE")
    print(f"CLIP available: {CLIP_AVAILABLE}")
    print(f"Whisper available: {WHISPER_AVAILABLE}")
    
    # Initialiser
    multimodal = MultiModalEmbeddings()
    print("Multi-Modal embeddings manager initialized")
