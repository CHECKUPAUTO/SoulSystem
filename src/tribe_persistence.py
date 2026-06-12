#!/usr/bin/env python3
"""TRIBE Persistence - Sauvegarde et chargement des données BrainStore"""
import json
import os
import pickle
import threading
import logging
from pathlib import Path
from datetime import datetime
from typing import Dict, Any, Optional

logger = logging.getLogger(__name__)

class TribePersistence:
    """Gère la persistance des données BrainStore sur disque"""
    
    def __init__(self, cache_dir: str = "/mnt/nvme/tribe_cache"):
        self.cache_dir = Path(cache_dir)
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self.entries_file = self.cache_dir / "brain_entries.pkl"
        self.metadata_file = self.cache_dir / "brain_metadata.json"
        self._lock = threading.Lock()
        
    def save_entries(self, entries: Dict[int, Any]) -> bool:
        """Sauvegarde les entrées BrainStore sur disque"""
        try:
            with self._lock:
                with open(self.entries_file, 'wb') as f:
                    pickle.dump(entries, f)
                metadata = {
                    "count": len(entries),
                    "last_save": datetime.now().isoformat(),
                    "version": "1.0"
                }
                with open(self.metadata_file, 'w') as f:
                    json.dump(metadata, f, indent=2)
                logger.info(f"Persisted {len(entries)} entries")
                return True
        except Exception as e:
            logger.error(f"Failed to save entries: {e}")
            return False
    
    def load_entries(self) -> Optional[Dict[int, Any]]:
        """Charge les entrées BrainStore depuis le disque"""
        try:
            if not self.entries_file.exists():
                logger.info("No persisted entries found")
                return None
            with self._lock:
                with open(self.entries_file, 'rb') as f:
                    entries = pickle.load(f)
                logger.info(f"Loaded {len(entries)} entries from disk")
                return entries
        except Exception as e:
            logger.error(f"Failed to load entries: {e}")
            return None
    
    def get_metadata(self) -> Optional[Dict[str, Any]]:
        """Récupère les métadonnées de la sauvegarde"""
        try:
            if not self.metadata_file.exists():
                return None
            with open(self.metadata_file, 'r') as f:
                return json.load(f)
        except Exception as e:
            logger.error(f"Failed to load metadata: {e}")
            return None

persistence = TribePersistence()
