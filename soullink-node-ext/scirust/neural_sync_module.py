import torch
import faiss
import numpy as np
from typing import Tuple, Optional

class NeuralStore:
    """Gestion hybride de la mémoire épisodique."""
    def __init__(self, dim: int = 128):
        self.dim = dim
        self.device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
        # Index FAISS CPU : la recherche vectorielle est rapide sur CPU
        self.index = faiss.IndexFlatL2(dim)
        
    def add_memory(self, vector: torch.Tensor):
        """Ajoute un état latent au store (passage vers CPU pour FAISS)."""
        vec_np = vector.detach().cpu().numpy().astype('float32')
        self.index.add(vec_np)

    def retrieve(self, query_state: torch.Tensor, k: int = 5) -> np.ndarray:
        """Récupère les K voisins les plus proches."""
        query_np = query_state.detach().cpu().numpy().astype('float32')
        _, indices = self.index.search(query_np, k)
        return indices

class BCIController:
    """Logique de contrôle pour la boucle de rétroaction BCI."""
    def __init__(self, sync_threshold: float = 0.3):
        self.sync_threshold = sync_threshold
        self.device = torch.device('cuda')

    def calculate_alpha_sync(self, current_state: torch.Tensor, memory_neighbors: torch.Tensor) -> float:
        """
        Calcule la synchronicité (α_sync).
        Plus l'état courant est similaire aux souvenirs, plus l'entropie diminue.
        """
        # Calcul de la similarité cosinus (produit scalaire normalisé)
        current_state = current_state / current_state.norm(p=2, dim=-1, keepdim=True)
        memory_neighbors = memory_neighbors / memory_neighbors.norm(p=2, dim=-1, keepdim=True)
        
        sim = torch.matmul(current_state, memory_neighbors.transpose(-2, -1))
        # Moyenne de la similarité : α_sync proche de 1 = forte cohérence
        alpha = torch.mean(sim).item()
        return alpha

    def check_trigger(self, alpha: float) -> bool:
        """Déclenche la boucle BCI si la synchronicité est en dessous du seuil critique."""
        # Si alpha est trop bas, le système est désynchronisé (entropie maximale)
        return alpha < self.sync_threshold

# Exemple d'usage dans la boucle principale
if __name__ == "__main__":
    store = NeuralStore()
    bci = BCIController(sync_threshold=0.3)
    
    # Simulation d'un état latent (GPU)
    latent_vec = torch.randn(1, 128).to('cuda')
    
    # Calcul de synchronicité
    alpha = bci.calculate_alpha_sync(latent_vec, latent_vec) # Simplified
    
    if bci.check_trigger(alpha):
        print(f"ALERTE : Dérive détectée (α={alpha:.2f}). Activation BCI.")
    else:
        print(f"Stabilité maintenue (α={alpha:.2f}).")
