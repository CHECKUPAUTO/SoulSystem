#!/usr/bin/env python3
"""
TRIBE Mass Ingestion Campaign - Simplified Version
But: Ingestion rapide de documents dans TRIBE Brain
"""

import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
import requests

# Configuration
TRIBE_URL = "http://localhost:7430"
DEFAULT_TARGET = 100_000
MAX_WORKERS = 8  # Réduit car TRIBE est CPU-intensive

# Thèmes
TOPICS = [
    "machine learning", "deep learning", "transformers", "NLP", 
    "computer vision", "reinforcement learning", "AI ethics",
    "optimization", "algorithms", "data science", "cloud computing",
    "cybersecurity", "blockchain", "IoT", "robotics", "genomics",
    "climate modeling", "quantum computing", "distributed systems",
    "neural networks", "computer graphics", "bioinformatics",
    "software engineering", "databases", "cryptography"
]

TEMPLATES = [
    "Research on {topic} shows significant progress. New methods combining {topic2} with {technique} achieve state-of-the-art results on {dataset}.",
    "This paper presents a novel approach to {topic}. We introduce a framework integrating {topic2} with traditional methods. Results show {pct}% improvement.",
    "The field of {topic} faces challenges in {challenge}. Our solution addresses this through {technique}. Evaluation on {n} datasets validates our approach.",
    "Recent advances in {topic} enable new applications in {domain}. We survey {n} papers identifying key trends in {technique}.",
    "We investigate {topic} theoretically. Connections to {topic2} are proven under {condition}. Experiments confirm our predictions."
]

TECHNIQUES = ["attention mechanisms", "graph neural networks", "adversarial training", "self-supervised learning", "knowledge distillation"]
DATASETS = ["ImageNet", "COCO", "Wikipedia", "arXiv", "PubMed", "GLUE", "MTEB"]
CHALLENGES = ["scalability", "interpretability", "generalization", "efficiency"]
DOMAINS = ["healthcare", "finance", "autonomous driving", "education"]
CONDITIONS = ["Lipschitz continuity", "strong convexity", "Markov assumptions"]

def generate_doc(doc_id: int) -> dict:
    """Génère un document synthétique."""
    import random
    topic = random.choice(TOPICS)
    template = random.choice(TEMPLATES)
    
    text = template.format(
        topic=topic,
        topic2=random.choice(TOPICS),
        technique=random.choice(TECHNIQUES),
        dataset=random.choice(DATASETS),
        pct=random.randint(10, 50),
        challenge=random.choice(CHALLENGES),
        n=random.randint(5, 500),
        domain=random.choice(DOMAINS),
        condition=random.choice(CONDITIONS)
    )
    
    return {
        "doc_id": doc_id,
        "text": text,
        "metadata": {
            "type": "synthetic",
            "topic": topic,
            "word_count": len(text.split()),
            "created_at": time.strftime("%Y-%m-%dT%H:%M:%S")
        }
    }

def ingest_doc(doc: dict) -> dict:
    """Ingest un document dans TRIBE."""
    try:
        resp = requests.post(
            f"{TRIBE_URL}/index/text",
            json=doc,
            timeout=10
        )
        return {
            "success": resp.status_code == 200,
            "doc_id": doc["doc_id"],
            "status": resp.status_code
        }
    except Exception as e:
        return {"success": False, "doc_id": doc["doc_id"], "error": str(e)[:50]}

def main():
    target = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_TARGET
    workers = int(sys.argv[2]) if len(sys.argv) > 2 else MAX_WORKERS
    
    print(f"🧠 TRIBE Mass Ingestion Campaign")
    print(f"   Target: {target:,} documents")
    print(f"   Workers: {workers}")
    print(f"   TRIBE URL: {TRIBE_URL}")
    print()
    
    # Vérifier TRIBE
    try:
        r = requests.get(f"{TRIBE_URL}/index/list", timeout=5)
        if r.status_code == 200:
            stats = r.json()
            print(f"📊 Documents actuels: {stats.get('n_chunks', 0):,}")
    except Exception as e:
        print(f"❌ TRIBE non accessible: {e}")
        return 1
    
    print(f"\n🚀 Démarrage...")
    start = time.time()
    success = fail = 0
    
    with ThreadPoolExecutor(max_workers=workers) as exe:
        futures = {exe.submit(ingest_doc, generate_doc(i)): i for i in range(target)}
        
        for i, fut in enumerate(as_completed(futures)):
            result = fut.result()
            if result["success"]:
                success += 1
            else:
                fail += 1
            
            if (i + 1) % 1000 == 0 or (i + 1) == target:
                elapsed = time.time() - start
                rate = (i + 1) / elapsed if elapsed > 0 else 0
                pct = 100 * (i + 1) / target
                print(f"   [{pct:5.1f}%] {i+1:,}/{target:,} | ✓ {success:,} ✗ {fail:,} | {rate:.1f} docs/s")
    
    elapsed = time.time() - start
    print(f"\n{'='*60}")
    print(f"✅ Terminé!")
    print(f"   Total: {success + fail:,}")
    print(f"   Succès: {success:,} ({100*success/(success+fail):.1f}%)")
    print(f"   Échecs: {fail:,}")
    print(f"   Temps: {elapsed/60:.1f} min")
    print(f"   Débit: {(success+fail)/elapsed:.1f} docs/s")
    print(f"{'='*60}")
    
    # Stats finales
    try:
        r = requests.get(f"{TRIBE_URL}/index/list", timeout=10)
        if r.status_code == 200:
            stats = r.json()
            print(f"\n📊 TRIBE Brain:")
            print(f"   Documents: {stats.get('n_documents', 0):,}")
            print(f"   Chunks: {stats.get('n_chunks', 0):,}")
    except:
        pass
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
