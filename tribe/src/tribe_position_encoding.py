"""
Position Encoding pour TRIBE
Implémentation sinusoidal + learned position embeddings
"""

import torch
import math
import numpy as np

class PositionEncoding:
    """
    Position encoding sinusoidal + learned pour les embeddings TRIBE.
    
    Usage:
        pe = PositionEncoding(d_model=1024, max_len=512)
        encoded = pe(embeddings)
    """
    
    def __init__(self, d_model: int = 1024, max_len: int = 512, dropout: float = 0.1):
        super().__init__()
        self.d_model = d_model
        self.max_len = max_len
        self.dropout = torch.nn.Dropout(p=dropout)
        
        # Créer le position encoding
        pe = torch.zeros(max_len, d_model)
        position = torch.arange(0, max_len, dtype=torch.float).unsqueeze(1)
        div_term = torch.exp(torch.arange(0, d_model, 2).float() * (-math.log(10000.0) / d_model))
        
        pe[:, 0::2] = torch.sin(position * div_term)
        pe[:, 1::2] = torch.cos(position * div_term)
        
        # Ajouter dimension batch
        pe = pe.unsqueeze(0)
        
        self.register_buffer('pe', pe)
    
    def forward(self, x: torch.Tensor) -> torch.Tensor:
        """
        Ajoute le position encoding aux embeddings.
        
        Args:
            x: Tensor de forme [batch_size, seq_len, d_model]
        
        Returns:
            Tensor avec position encoding ajouté
        """
        seq_len = x.size(1)
        if seq_len > self.max_len:
            # Tronquer si séquence trop longue
            x = x[:, :self.max_len, :]
            seq_len = self.max_len
        
        return self.dropout(x + self.pe[:, :seq_len, :])


class LearnedPositionEncoding(torch.nn.Module):
    """
    Position encoding appris (learned) pour TRIBE.
    Alternative au sinusoidal avec des embeddings appris.
    """
    
    def __init__(self, d_model: int = 1024, max_len: int = 512, dropout: float = 0.1):
        super().__init__()
        self.d_model = d_model
        self.max_len = max_len
        self.dropout = torch.nn.Dropout(p=dropout)
        
        # Embeddings de position appris
        self.position_embeddings = torch.nn.Embedding(max_len, d_model)
        self.position_embeddings.weight.data.normal_(mean=0.0, std=0.02)
    
    def forward(self, x: torch.Tensor) -> torch.Tensor:
        seq_len = x.size(1)
        if seq_len > self.max_len:
            x = x[:, :self.max_len, :]
            seq_len = self.max_len
        
        positions = torch.arange(seq_len, device=x.device).unsqueeze(0)
        position_embeddings = self.position_embeddings(positions)
        
        return self.dropout(x + position_embeddings)


def add_position_encoding(embeddings: np.ndarray, max_len: int = 512) -> np.ndarray:
    """
    Fonction utilitaire pour ajouter le position encoding aux embeddings.
    
    Args:
        embeddings: Array de forme [seq_len, d_model] ou [batch, seq_len, d_model]
        max_len: Longueur maximale de la séquence
    
    Returns:
        Embeddings avec position encoding ajouté
    """
    # Convertir en tensor si nécessaire
    if isinstance(embeddings, np.ndarray):
        embeddings = torch.from_numpy(embeddings).float()
    
    # Ajouter dimension batch si nécessaire
    if embeddings.dim() == 2:
        embeddings = embeddings.unsqueeze(0)
    
    # Créer position encoding
    batch_size, seq_len, d_model = embeddings.shape
    
    # Position encoding sinusoidal
    position = torch.arange(seq_len).unsqueeze(1)
    div_term = torch.exp(torch.arange(0, d_model, 2).float() * (-math.log(10000.0) / d_model))
    
    pe = torch.zeros(seq_len, d_model)
    pe[:, 0::2] = torch.sin(position * div_term)
    pe[:, 1::2] = torch.cos(position * div_term)
    
    # Ajouter aux embeddings
    encoded = embeddings + pe.unsqueeze(0)
    
    return encoded.numpy()


# Test
if __name__ == "__main__":
    # Test avec embeddings aléatoires
    batch_size = 4
    seq_len = 128
    d_model = 1024
    
    # Embeddings aléatoires
    embeddings = torch.randn(batch_size, seq_len, d_model)
    
    # Position encoding sinusoidal
    pe_sinusoidal = PositionEncoding(d_model=d_model, max_len=512)
    encoded_sin = pe_sinusoidal(embeddings)
    
    # Position encoding appris
    pe_learned = LearnedPositionEncoding(d_model=d_model, max_len=512)
    encoded_learned = pe_learned(embeddings)
    
    print(f"Input shape: {embeddings.shape}")
    print(f"Sinusoidal output shape: {encoded_sin.shape}")
    print(f"Learned output shape: {encoded_learned.shape}")
    print(f"Position encoding prêt!")
