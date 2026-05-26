#!/usr/bin/env python3
"""
observe_real.py — Phase 1 Observation réelle avec Qwen2.5-0.5B

Observe 50 prompts, stocke les embeddings réels dans le SemanticStore,
analyse les clusters. Résultat : cluster_report.json
"""
import json
import sys
import urllib.request
import numpy as np
from sklearn.cluster import KMeans
from sklearn.metrics import silhouette_score
from datetime import datetime

EMBED_URL = "http://localhost:9097/embed"
PROMPTS = [
    "Explain recursion in programming",
    "What is a neural network?",
    "Write a poem about Rust",
    "How does gradient descent work?",
    "Define quantum computing",
    "What is the meaning of life?",
    "Explain TCP/IP protocol",
    "Write a haiku about AI",
    "How do transformers work?",
    "What is backpropagation?",
    "Explain blockchain technology",
    "What is the Pythagorean theorem?",
    "Describe the water cycle",
    "What is machine learning?",
    "How does GPS work?",
    "Explain DNA replication",
    "What is the capital of Japan?",
    "How to write a binary search",
    "Explain general relativity",
    "What is entropy?",
    "Describe photosynthesis",
    "What is an API?",
    "Explain the stock market",
    "How does encryption work?",
    "What is the Turing test?",
    "Explain natural selection",
    "What is a hash function?",
    "How do CPUs work?",
    "Define artificial intelligence",
    "What is the singularity?",
    "Explain the Big Bang",
    "How does WiFi work?",
    "What is a database index?",
    "Describe the human genome",
    "What is time complexity?",
    "Explain the Doppler effect",
    "How does cryptography work?",
    "What is the Heisenberg principle?",
    "Describe the solar system",
    "What is an operating system?",
    "Explain the Krebs cycle",
    "How do batteries work?",
    "What is the Monte Carlo method?",
    "Describe cloud computing",
    "What is a blockchain fork?",
    "Explain the Higgs boson",
    "How does the internet work?",
    "What is a qubit?",
    "Describe the food chain",
    "What is the Fibonacci sequence?",
]

def get_embedding(text):
    data = json.dumps({"input": text}).encode()
    req = urllib.request.Request(EMBED_URL, data=data,
        headers={"Content-Type": "application/json"})
    resp = urllib.request.urlopen(req, timeout=30)
    result = json.loads(resp.read())
    emb = result.get("embedding")
    if emb and isinstance(emb, list) and len(emb) > 0:
        # If it's a list of lists (multi-token), take mean
        if isinstance(emb[0], list):
            return [sum(x)/len(x) for x in zip(*emb)]
        return emb
    return None

def main():
    print("=" * 60)
    print("PHASE 1 - OBSERVATION LLM RÉELLE")
    print(f"Modèle: Qwen2.5-0.5B-Instruct Q4_K_M")
    print(f"Prompts: {len(PROMPTS)}")
    print(f"Serveur: {EMBED_URL}")
    print("=" * 60)

    # Collect embeddings
    embeddings = []
    successful = 0
    errors = 0
    start = datetime.now()

    for i, prompt in enumerate(PROMPTS):
        try:
            emb = get_embedding(prompt)
            if emb and len(emb) > 10:  # sanity check
                embeddings.append(emb)
                successful += 1
                energy = sum(x*x for x in emb)**0.5
                print(f"  [{i+1:2d}/{len(PROMPTS)}] energy={energy:.2f} dim={len(emb)}")
            else:
                errors += 1
                print(f"  [{i+1:2d}/{len(PROMPTS)}] ❌ réponse vide ou trop petite")
        except Exception as e:
            errors += 1
            print(f"  [{i+1:2d}/{len(PROMPTS)}] ❌ {e}")

    elapsed = (datetime.now() - start).total_seconds()
    print()
    print(f"Résultats: {successful} succès, {errors} erreurs, {elapsed:.1f}s")
    print()

    if successful < 2:
        print("❌ Pas assez d'embeddings pour l'analyse")
        return

    X = np.array(embeddings)
    print(f"Shape: {X.shape}")
    print(f"Dimensions: {X.shape[1]}")
    print()

    # Analyse clusters
    n_clusters = min(5, successful)
    kmeans = KMeans(n_clusters=n_clusters, random_state=42, n_init=10)
    labels = kmeans.fit_predict(X)

    print(f"Clusters (k={n_clusters}):")
    for c in range(n_clusters):
        count = sum(1 for l in labels if l == c)
        members = [PROMPTS[i] for i in range(len(labels)) if labels[i] == c]
        print(f"  Cluster {c}: {count} prompts")
        for m in members[:3]:
            print(f"    - {m}")
        if count > 3:
            print(f"    ... et {count-3} autres")
    print()

    # Métriques
    intra = 0
    for c in range(n_clusters):
        pts = X[labels == c]
        if len(pts) > 1:
            centroid = kmeans.cluster_centers_[c]
            dists = np.linalg.norm(pts - centroid, axis=1)
            intra += dists.mean()
    intra /= n_clusters

    centroids = kmeans.cluster_centers_
    inter = 0
    count = 0
    for i in range(n_clusters):
        for j in range(i+1, n_clusters):
            d = np.linalg.norm(centroids[i] - centroids[j])
            inter += d
            count += 1
    inter /= max(count, 1)

    print(f"Métriques:")
    print(f"  Distance intra-cluster (avg): {intra:.4f}")
    print(f"  Distance inter-cluster (avg): {inter:.4f}")
    print(f"  Ratio inter/intra: {inter/max(intra,0.001):.4f}")
    print()

    # Rapport
    report = {
        "timestamp": datetime.now().isoformat(),
        "model": "Qwen2.5-0.5B-Instruct.Q4_K_M",
        "total_prompts": len(PROMPTS),
        "successful": successful,
        "errors": errors,
        "elapsed_seconds": elapsed,
        "embedding_dim": X.shape[1],
        "n_clusters": n_clusters,
        "intra_cluster_distance": round(float(intra), 4),
        "inter_cluster_distance": round(float(inter), 4),
        "separation_ratio": round(float(inter/max(intra,0.001)), 4),
        "cluster_membership": {str(c): [PROMPTS[i] for i in range(len(labels)) if labels[i] == c] for c in range(n_clusters)},
        "verdict": "✅ CLUSTERS BIEN FORMÉS" if inter > intra * 2 else "⚠️ CLUSTERS PARTIELS"
    }

    with open("/root/SoulSystem/scirust-chronos-agent/data/observation_report.json", "w") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
    print(f"📄 Rapport sauvegardé: observation_report.json")
    print(f"🎯 Verdict: {report['verdict']}")

if __name__ == "__main__":
    main()
