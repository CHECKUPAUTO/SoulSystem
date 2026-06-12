#!/usr/bin/env python3
"""
Sauvegarde automatique du Brain vers NVMe
Exporte l'état via API et sauvegarde sur disque
"""
import json
import time
import requests
from datetime import datetime
from pathlib import Path

BASE_PATH = Path("/mnt/nvme/soullink_brain")
NEURONS_PATH = BASE_PATH / "neurons"
SYNAPSES_PATH = BASE_PATH / "synapses"
TOPICS_PATH = BASE_PATH / "topics"
MEMORIES_PATH = BASE_PATH / "memories"
BACKUPS_PATH = BASE_PATH / "backups"

BRAIN_URL = "http://localhost:8084"

def save_brain_state():
    """Sauvegarde l'état complet du Brain."""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    
    # Récupérer l'état via API
    try:
        status = requests.get(f"{BRAIN_URL}/api/status", timeout=5).json()
        brain = requests.get(f"{BRAIN_URL}/api/brain", timeout=5).json()
    except Exception as e:
        print(f"❌ Erreur API: {e}")
        return False
    
    # Sauvegarder les neurones
    neurons_file = NEURONS_PATH / f"neurons_{timestamp}.json"
    with open(neurons_file, 'w') as f:
        json.dump(brain["neurons"], f, indent=2)
    
    # Sauvegarder les synapses
    synapses_file = SYNAPSES_PATH / f"synapses_{timestamp}.json"
    with open(synapses_file, 'w') as f:
        json.dump(brain["synapses"], f, indent=2)
    
    # Sauvegarder le statut
    status_file = BASE_PATH / f"status_{timestamp}.json"
    with open(status_file, 'w') as f:
        json.dump(status, f, indent=2)
    
    # Nettoyer les anciens fichiers (garder 10)
    cleanup_old_files(NEURONS_PATH, "neurons", keep=10)
    cleanup_old_files(SYNAPSES_PATH, "synapses", keep=10)
    cleanup_old_files(BASE_PATH, "status", keep=10)
    
    print(f"💾 {datetime.now().strftime('%H:%M:%S')} - "
          f"Neurones: {status['stats']['total_neurons']}, "
          f"Synapses: {status['stats']['total_synapses']}, "
          f"Topics: {status['stats']['neuroscience_topics_learned']}")
    
    return True

def save_learned_topics(topics: list):
    """Sauvegarde les topics appris."""
    topics_file = TOPICS_PATH / "learned_topics.json"
    with open(topics_file, 'w') as f:
        json.dump(topics, f, indent=2)

def create_backup():
    """Crée un backup complet."""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_dir = BACKUPS_PATH / f"backup_{timestamp}"
    backup_dir.mkdir(parents=True, exist_ok=True)
    
    # Copier les derniers fichiers
    for path, name in [(NEURONS_PATH, "neurons"), (SYNAPSES_PATH, "synapses")]:
        files = sorted(path.glob(f"{name}_*.json"), reverse=True)[:1]
        for f in files:
            import shutil
            shutil.copy2(f, backup_dir / f.name)
    
    # Sauvegarder le statut
    try:
        status = requests.get(f"{BRAIN_URL}/api/status", timeout=5).json()
        with open(backup_dir / "brain_state.json", 'w') as f:
            json.dump(status, f, indent=2)
    except:
        pass
    
    # Nettoyer les anciens backups (garder 5)
    backups = sorted(BACKUPS_PATH.glob("backup_*"))
    while len(backups) > 5:
        import shutil
        shutil.rmtree(backups[0])
        backups = backups[1:]
    
    print(f"📦 Backup créé: {backup_dir}")

def cleanup_old_files(path: Path, prefix: str, keep: int = 10):
    """Nettoie les anciens fichiers."""
    files = sorted(path.glob(f"{prefix}_*.json"), reverse=True)
    for f in files[keep:]:
        f.unlink()

def get_storage_stats():
    """Retourne les statistiques de stockage."""
    total_size = 0
    file_counts = {}
    
    for path, name in [
        (NEURONS_PATH, "neurons"),
        (SYNAPSES_PATH, "synapses"),
        (TOPICS_PATH, "topics"),
        (BACKUPS_PATH, "backups"),
    ]:
        files = list(path.glob("*.json"))
        file_counts[name] = len(files)
        total_size += sum(f.stat().st_size for f in files)
    
    import os
    stat = os.statvfs(BASE_PATH)
    free_gb = (stat.f_bavail * stat.f_frsize) / (1024**3)
    
    return {
        "total_files": sum(file_counts.values()),
        "file_counts": file_counts,
        "storage_used_mb": round(total_size / (1024**2), 2),
        "free_space_gb": round(free_gb, 2),
        "allocated_gb": 100,
    }

if __name__ == "__main__":
    print("🧠 SoulLink Brain Auto-Save — NVMe Storage")
    print("💾 Sauvegarde toutes les 5 minutes")
    print("📦 Backup complet toutes les 6 heures")
    print()
    
    last_backup = 0
    save_count = 0
    
    while True:
        try:
            save_brain_state()
            save_count += 1
            
            # Backup complet toutes les 6 heures (72 sauvegardes)
            if save_count % 72 == 0:
                create_backup()
                stats = get_storage_stats()
                print(f"📊 Storage: {stats['storage_used_mb']} MB used, {stats['free_space_gb']} GB free")
            
        except Exception as e:
            print(f"❌ Erreur: {e}")
        
        time.sleep(300)  # 5 minutes