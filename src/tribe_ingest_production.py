#!/usr/bin/env python3
"""
TRIBE Mass Ingestion Campaign - Production Version
Ingestion massive avec batching et monitoring
"""

import sys
import time
import random
from concurrent.futures import ThreadPoolExecutor, as_completed
import requests
import json

# Configuration
TRIBE_URL = "http://localhost:7430"
BATCH_SIZE = 100  # Nombre de documents avant stat
LOG_EVERY = 50

def short_doc(doc_id: int) -> dict:
    """Génère un document court et varié."""
    topics = ["ML", "DL", "AI", "NLP", "CV", "RL", "Crypto", "DB", "OS", "Net", "Sec", "Cloud"]
    aspects = ["optimization", "architecture", "training", "inference", "scaling", "privacy", "ethics"]
    
    topic = random.choice(topics)
    aspect = random.choice(aspects)
    
    templates = [
        f"{topic} {aspect}: Advances in {topic.lower()} enable better {aspect} through novel approaches.",
        f"Research {topic}: New methods improve {aspect} in {topic.lower()} systems by 20-40%.",
        f"{topic} survey: Analysis of {aspect} techniques in {topic.lower()} shows promising results.",
        f"{topic} benchmark: Performance evaluation of {aspect} methods on standard datasets.",
        f"Future of {topic}: Emerging trends in {aspect} reshape {topic.lower()} landscape.",
        f"{topic} framework: Unified approach to {aspect} across different {topic.lower()} models.",
        f"{topic} theory: Mathematical foundations of {aspect} in {topic.lower()} explained.",
        f"Applied {topic}: Real-world deployment of {aspect} solutions in industry.",
        f"{topic} tutorial: Step-by-step guide to implementing {aspect} techniques.",
        f"{topic} critique: Analysis of limitations in current {aspect} approaches.",
    ]
    
    return {
        "doc_id": doc_id,
        "text": random.choice(templates),
        "metadata": {
            "type": "auto",
            "topic": topic,
            "aspect": aspect,
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S")
        }
    }

def ingest(doc: dict) -> dict:
    """Ingest un document."""
    try:
        r = requests.post(
            f"{TRIBE_URL}/index/text",
            json=doc,
            timeout=15
        )
        return {
            "ok": r.status_code == 200,
            "id": doc["doc_id"],
            "code": r.status_code
        }
    except Exception as e:
        return {"ok": False, "id": doc["doc_id"], "err": str(e)[:40]}

def main():
    target = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
    workers = int(sys.argv[2]) if len(sys.argv) > 2 else 16
    
    print(f"🧠 TRIBE Mass Ingestion")
    print(f"   Target: {target:,} docs")
    print(f"   Workers: {workers}")
    print(f"   URL: {TRIBE_URL}")
    print()
    
    # Stats initiales
    try:
        r = requests.get(f"{TRIBE_URL}/index/list", timeout=5)
        if r.status_code == 200:
            s = r.json()
            print(f"📊 Actuel: {s.get('n_chunks',0):,} chunks, {s.get('n_documents',0):,} docs")
    except:
        pass
    
    print(f"\n🚀 Démarrage...")
    print(f"   (Pressez Ctrl+C pour arrêter proprement)")
    print()
    
    start = time.time()
    success = fail = 0
    last_log = 0
    
    with ThreadPoolExecutor(max_workers=workers) as exe:
        futures = {exe.submit(ingest, short_doc(i)): i for i in range(target)}
        
        for i, fut in enumerate(as_completed(futures)):
            r = fut.result()
            if r["ok"]:
                success += 1
            else:
                fail += 1
            
            # Log progress
            processed = i + 1
            if processed - last_log >= LOG_EVERY or processed == target:
                elapsed = time.time() - start
                rate = processed / elapsed if elapsed > 0 else 0
                pct = 100 * processed / target
                print(f"[{pct:5.1f}%] {processed:,}/{target:,} | ✓{success:,} ✗{fail:,} | {rate:.1f}/s | {elapsed/60:.1f}min")
                last_log = processed
    
    # Résumé
    elapsed = time.time() - start
    print(f"\n{'='*50}")
    print(f"✅ Terminé!")
    print(f"   Total: {success+fail:,}")
    print(f"   Succès: {success:,} ({100*success/(success+fail):.1f}%)")
    print(f"   Temps: {elapsed/60:.1f} min")
    print(f"   Débit: {(success+fail)/elapsed:.1f} docs/s")
    print(f"{'='*50}")
    
    # Stats finales
    try:
        r = requests.get(f"{TRIBE_URL}/index/list", timeout=10)
        if r.status_code == 200:
            s = r.json()
            print(f"\n📊 Final: {s.get('n_chunks',0):,} chunks, {s.get('n_documents',0):,} docs")
    except:
        pass

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\n⚠️ Arrêté par l'utilisateur")
        sys.exit(1)
