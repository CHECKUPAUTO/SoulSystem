#!/usr/bin/env python3
"""
TRIBE Mass Ingestion Campaign
But: Ingestion de 100 000 documents dans TRIBE Brain (port 7440)
Sources: arXiv, PubMed, fichiers locaux, données synthétiques
"""

import asyncio
import json
import random
import time
from pathlib import Path
from typing import List, Dict, Any
try:
    import aiohttp
    HAS_AIOHTTP = True
except ImportError:
    HAS_AIOHTTP = False
from concurrent.futures import ThreadPoolExecutor, as_completed
import requests

# Configuration
TRIBE_URL = "http://localhost:7440"
MSA_URL = "http://localhost:7430"
TARGET_DOCS = 100_000
BATCH_SIZE = 50
MAX_WORKERS = 16

# Thèmes pour génération de contenu
TOPICS = [
    "machine learning", "deep learning", "neural networks", "transformers",
    "natural language processing", "computer vision", "reinforcement learning",
    "quantum computing", "blockchain", "cybersecurity", "data science",
    "big data", "cloud computing", "edge computing", "IoT",
    "robotics", "autonomous systems", "computer graphics", "human-computer interaction",
    "bioinformatics", "computational biology", "genomics", "proteomics",
    "climate modeling", "astrophysics", "materials science", "nanotechnology",
    "optimization algorithms", "graph theory", "linear algebra", "statistics",
    "software engineering", "distributed systems", "databases", "operating systems",
    "programming languages", "compilers", "formal methods", "type theory",
    "cryptography", "privacy", "ethics in AI", "fairness", "explainable AI"
]

def generate_synthetic_document(topic: str, doc_id: int) -> Dict[str, Any]:
    """Génère un document synthétique réaliste sur un thème."""
    templates = [
        f"Research on {topic} has shown significant progress in recent years. "
        f"New methodologies combining {random.choice(TOPICS)} with {topic} "
        f"have demonstrated improved performance metrics across multiple benchmarks. "
        f"The approach leverages {random.choice(['attention mechanisms', 'graph neural networks', 'adversarial training', 'self-supervised learning'])} "
        f"to achieve state-of-the-art results. Experimental validation on {random.choice(['ImageNet', 'COCO', 'Wikipedia', 'PubMed', 'arXiv'])} "
        f"confirms the effectiveness of this methodology.",
        
        f"This paper presents a novel approach to {topic}. We introduce a framework that "
        f"integrates {random.choice(TOPICS)} techniques with traditional {topic} methods. "
        f"Our contributions include: (1) a unified architecture for {topic}, "
        f"(2) an optimization algorithm for {random.choice(['latency', 'throughput', 'accuracy', 'robustness'])}, "
        f"and (3) extensive empirical evaluation. Results show {random.randint(10, 50)}% improvement over baselines.",
        
        f"The field of {topic} faces challenges including {random.choice(['scalability', 'interpretability', 'generalization', 'efficiency'])}. "
        f"We propose solutions based on {random.choice(TOPICS)} principles. "
        f"Our method addresses {random.choice(['memory constraints', 'computational complexity', 'data scarcity', 'distribution shift'])} "
        f"through innovative use of {random.choice(['knowledge distillation', 'model compression', 'data augmentation', 'transfer learning'])}. "
        f"Evaluation on {random.randint(5, 20)} datasets validates our claims.",
        
        f"Recent advances in {topic} have enabled new applications in {random.choice(['healthcare', 'finance', 'autonomous driving', 'education', 'manufacturing'])}. "
        f"However, {random.choice(['bias', 'privacy concerns', 'security vulnerabilities', 'environmental impact'])} remain open problems. "
        f"We survey {random.randint(50, 500)} papers and identify key trends. "
        f"Our analysis reveals that {random.choice(['ensemble methods', 'modular architectures', 'multi-modal approaches', 'continual learning'])} "
        f"show the most promise for future research.",
        
        f"We investigate {topic} from a theoretical perspective. Our analysis reveals "
        f"fundamental connections between {topic} and {random.choice(TOPICS)}. "
        f"We prove that {random.choice(['convergence is guaranteed', 'the bound is tight', 'the complexity is optimal', 'the representation is complete'])} "
        f"under {random.choice(['Lipschitz continuity', 'strong convexity', 'Markov assumptions', 'i.i.d. sampling'])}. "
        f"Experiments validate our theoretical predictions.",
    ]
    
    text = random.choice(templates)
    
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

def ingest_document_sync(doc: Dict[str, Any]) -> Dict[str, Any]:
    """Ingest un document via l'API TRIBE (synchrone)."""
    try:
        response = requests.post(
            f"{TRIBE_URL}/index/text",
            json=doc,
            timeout=30
        )
        if response.status_code == 200:
            return {"success": True, "doc_id": doc["doc_id"], "response": response.json()}
        else:
            return {"success": False, "doc_id": doc["doc_id"], "error": response.text}
    except Exception as e:
        return {"success": False, "doc_id": doc["doc_id"], "error": str(e)}

async def ingest_batch_async(docs: List[Dict[str, Any]], session) -> List[Dict[str, Any]]:
    """Ingest un batch de documents (asynchrone)."""
    tasks = []
    for doc in docs:
        task = session.post(f"{TRIBE_URL}/index/text", json=doc, timeout=aiohttp.ClientTimeout(total=30))
        tasks.append(task)
    
    results = []
    responses = await asyncio.gather(*tasks, return_exceptions=True)
    
    for doc, resp in zip(docs, responses):
        if isinstance(resp, Exception):
            results.append({"success": False, "doc_id": doc["doc_id"], "error": str(resp)})
        else:
            if resp.status == 200:
                results.append({"success": True, "doc_id": doc["doc_id"], "response": await resp.json()})
            else:
                results.append({"success": False, "doc_id": doc["doc_id"], "error": await resp.text()})
    
    return results

async def mass_ingestion_campaign():
    """Lance la campagne d'ingestion massive."""
    print(f"🧠 TRIBE Mass Ingestion Campaign")
    print(f"   Target: {TARGET_DOCS:,} documents")
    print(f"   Batch size: {BATCH_SIZE}")
    print(f"   Workers: {MAX_WORKERS}")
    print(f"   TRIBE URL: {TRIBE_URL}")
    print()
    
    # Vérifier TRIBE
    try:
        resp = requests.get(f"{TRIBE_URL}/index/list", timeout=5)
        if resp.status_code == 200:
            stats = resp.json()
            print(f"📊 Documents actuellement indexés: {stats.get('n_chunks', 0)}")
    except:
        print("⚠️  TRIBE non accessible, tentative de connexion...")
    
    # Générer les documents
    print(f"\n📝 Génération de {TARGET_DOCS:,} documents synthétiques...")
    all_docs = []
    for i in range(TARGET_DOCS):
        topic = random.choice(TOPICS)
        doc = generate_synthetic_document(topic, i + 1)
        all_docs.append(doc)
        
        if (i + 1) % 10000 == 0:
            print(f"   Générés: {i + 1:,} / {TARGET_DOCS:,}")
    
    print(f"✅ {len(all_docs):,} documents générés\n")
    
    # Ingestion par batches
    print(f"🚀 Démarrage de l'ingestion...")
    
    success_count = 0
    fail_count = 0
    start_time = time.time()
    
    async with aiohttp.ClientSession() as session:
        for batch_idx in range(0, len(all_docs), BATCH_SIZE):
            batch = all_docs[batch_idx:batch_idx + BATCH_SIZE]
            results = await ingest_batch_async(batch, session)
            
            for r in results:
                if r["success"]:
                    success_count += 1
                else:
                    fail_count += 1
            
            # Progress
            processed = batch_idx + len(batch)
            if processed % 1000 == 0 or processed == len(all_docs):
                elapsed = time.time() - start_time
                rate = processed / elapsed if elapsed > 0 else 0
                eta = (len(all_docs) - processed) / rate if rate > 0 else 0
                print(f"   Progress: {processed:,}/{len(all_docs):,} | "
                      f"✓ {success_count:,} ✗ {fail_count:,} | "
                      f"Rate: {rate:.1f} docs/sec | ETA: {eta/60:.1f} min")
    
    total_time = time.time() - start_time
    print(f"\n{'='*60}")
    print(f"✅ Campagne terminée!")
    print(f"   Total traités: {success_count + fail_count:,}")
    print(f"   Succès: {success_count:,}")
    print(f"   Échecs: {fail_count:,}")
    print(f"   Temps total: {total_time/60:.1f} minutes")
    print(f"   Débit moyen: {(success_count + fail_count)/total_time:.1f} docs/sec")
    print(f"{'='*60}")
    
    # Vérifier les stats finales
    try:
        resp = requests.get(f"{TRIBE_URL}/index/list", timeout=10)
        if resp.status_code == 200:
            stats = resp.json()
            print(f"\n📊 TRIBE Brain Stats:")
            print(f"   Documents: {stats.get('n_documents', 0):,}")
            print(f"   Chunks: {stats.get('n_chunks', 0):,}")
    except Exception as e:
        print(f"⚠️  Impossible de récupérer les stats finales: {e}")

if __name__ == "__main__":
    asyncio.run(mass_ingestion_campaign())
