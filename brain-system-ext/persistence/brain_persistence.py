#!/usr/bin/env python3
"""
SoulLink Brain Persistence — Sauvegarde et chargement depuis NVMe
100 Go de stockage dédié pour le cerveau
"""
import json
import os
import pickle
import time
from datetime import datetime
from pathlib import Path
import threading
import shutil

BASE_PATH = Path("/mnt/nvme/soullink_brain")
NEURONS_PATH = BASE_PATH / "neurons"
SYNAPSES_PATH = BASE_PATH / "synapses"
MEMORIES_PATH = BASE_PATH / "memories"
MODELS_PATH = BASE_PATH / "models"
TOPICS_PATH = BASE_PATH / "topics"
BACKUPS_PATH = BASE_PATH / "backups"

# Créer les répertoires si nécessaires
for path in [NEURONS_PATH, SYNAPSES_PATH, MEMORIES_PATH, MODELS_PATH, TOPICS_PATH, BACKUPS_PATH]:
    path.mkdir(parents=True, exist_ok=True)

class BrainPersistence:
    """Gestion de la persistance du cerveau sur NVMe."""
    
    def __init__(self):
        self.last_save = 0
        self.save_interval = 300  # 5 minutes
        self.backup_interval = 6 * 3600  # 6 heures
        self.running = True
        
        # Démarrer le thread de sauvegarde automatique
        self.save_thread = threading.Thread(target=self._auto_save_loop, daemon=True)
        self.save_thread.start()
    
    def save_neurons(self, neurons: list, module_name: str):
        """Sauvegarde les neurones d'un module."""
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = NEURONS_PATH / f"{module_name}_{timestamp}.json"
        
        data = []
        for neuron in neurons:
            data.append({
                "id": neuron.id,
                "layer": neuron.layer,
                "potential": round(neuron.potential, 4),
                "activation": round(neuron.activation, 4),
                "importance": round(neuron.importance, 4),
                "firing_count": neuron.firing_count,
                "x": round(neuron.x, 2),
                "y": round(neuron.y, 2),
            })
        
        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)
        
        # Nettoyer les anciens fichiers
        self._cleanup_old_files(NEURONS_PATH, module_name, keep=3)
    
    def save_synapses(self, synapses: list):
        """Sauvegarde les synapses."""
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = SYNAPSES_PATH / f"synapses_{timestamp}.json"
        
        data = []
        for synapse in synapses:
            if synapse.source and synapse.target:
                data.append({
                    "source": synapse.source.id,
                    "target": synapse.target.id,
                    "weight": round(synapse.weight, 4),
                    "hebbian_strength": round(synapse.hebbian_strength, 4),
                })
        
        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)
        
        self._cleanup_old_files(SYNAPSES_PATH, "synapses", keep=3)
    
    def save_topics(self, learned_topics: set):
        """Sauvegarde les topics appris."""
        filename = TOPICS_PATH / "learned_topics.json"
        
        with open(filename, 'w') as f:
            json.dump(list(learned_topics), f, indent=2)
    
    def load_topics(self) -> set:
        """Charge les topics appris."""
        filename = TOPICS_PATH / "learned_topics.json"
        
        if filename.exists():
            with open(filename, 'r') as f:
                return set(json.load(f))
        return set()
    
    def save_memory(self, key: str, value: dict, tags: list = None):
        """Sauvegarde une mémoire."""
        filename = MEMORIES_PATH / f"{key}.json"
        
        data = {
            "key": key,
            "value": value,
            "tags": tags or [],
            "timestamp": time.time(),
            "date": datetime.now().isoformat(),
        }
        
        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)
    
    def load_memory(self, key: str) -> dict:
        """Charge une mémoire."""
        filename = MEMORIES_PATH / f"{key}.json"
        
        if filename.exists():
            with open(filename, 'r') as f:
                return json.load(f)
        return None
    
    def create_backup(self, brain_state: dict):
        """Crée un backup complet du cerveau."""
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        backup_dir = BACKUPS_PATH / f"backup_{timestamp}"
        backup_dir.mkdir(parents=True, exist_ok=True)
        
        # Sauvegarder l'état complet
        state_file = backup_dir / "brain_state.json"
        with open(state_file, 'w') as f:
            json.dump(brain_state, f, indent=2)
        
        # Copier les fichiers récents
        for path in [NEURONS_PATH, SYNAPSES_PATH, TOPICS_PATH]:
            recent = list(path.glob("*.json"))[:1]  # Le plus récent
            for f in recent:
                shutil.copy2(f, backup_dir / f.name)
        
        # Nettoyer les anciens backups (garder 5)
        backups = sorted(BACKUPS_PATH.glob("backup_*"))
        while len(backups) > 5:
            shutil.rmtree(backups[0])
            backups = backups[1:]
        
        print(f"✅ Backup créé: {backup_dir}")
        return backup_dir
    
    def get_storage_stats(self) -> dict:
        """Retourne les statistiques de stockage."""
        total_size = 0
        file_counts = {}
        
        for path, name in [
            (NEURONS_PATH, "neurons"),
            (SYNAPSES_PATH, "synapses"),
            (MEMORIES_PATH, "memories"),
            (TOPICS_PATH, "topics"),
            (BACKUPS_PATH, "backups"),
        ]:
            files = list(path.glob("*.json"))
            file_counts[name] = len(files)
            total_size += sum(f.stat().st_size for f in files)
        
        # Vérifier l'espace disque
        stat = os.statvfs(BASE_PATH)
        free_gb = (stat.f_bavail * stat.f_frsize) / (1024**3)
        total_gb = (stat.f_blocks * stat.f_frsize) / (1024**3)
        
        return {
            "total_files": sum(file_counts.values()),
            "file_counts": file_counts,
            "storage_used_mb": round(total_size / (1024**2), 2),
            "free_space_gb": round(free_gb, 2),
            "total_space_gb": round(total_gb, 2),
            "allocated_gb": 100,
            "last_save": datetime.fromtimestamp(self.last_save).isoformat() if self.last_save else None,
        }
    
    def _cleanup_old_files(self, path: Path, prefix: str, keep: int = 3):
        """Nettoie les anciens fichiers."""
        files = sorted(path.glob(f"{prefix}_*.json"), reverse=True)
        for f in files[keep:]:
            f.unlink()
    
    def _auto_save_loop(self):
        """Boucle de sauvegarde automatique."""
        while self.running:
            time.sleep(60)  # Vérifier chaque minute
            
            # Sauvegarde automatique toutes les 5 minutes
            if time.time() - self.last_save > self.save_interval:
                # La sauvegarde sera faite par le brain principal
                pass
    
    def stop(self):
        """Arrête le thread de sauvegarde."""
        self.running = False

# Instance globale
PERSISTENCE = BrainPersistence()

if __name__ == "__main__":
    # Test
    stats = PERSISTENCE.get_storage_stats()
    print("📊 Storage Stats:")
    print(f"   Files: {stats['total_files']}")
    print(f"   Used: {stats['storage_used_mb']} MB")
    print(f"   Free: {stats['free_space_gb']} GB")
    print(f"   Allocated: {stats['allocated_gb']} GB")