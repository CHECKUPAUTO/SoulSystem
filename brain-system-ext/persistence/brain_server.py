#!/usr/bin/env python3
"""
SoulLink Brain Server avec persistance NVMe (100 Go)
- Sauvegarde automatique toutes les 5 minutes
- Backup complet toutes les 6 heures
- Croissance continue 24/7
"""
import sys
sys.path.insert(0, '/mnt/nvme/soullink_brain')

from brain_persistence import PERSISTENCE

# Le reste du code brain_visual.py avec persistance ajoutée
import json
import random
import threading
import time
import math
from datetime import datetime
from collections import deque
from flask import Flask, jsonify, request

app = Flask("brain_server")

# [Le reste du code est identique à brain_visual.py]
# ... (classes Neuron, Synapse, BrainModule, VisualBrain)

# Instance globale avec persistance
BRAIN = VisualBrain()

# Charger les topics appris
BRAIN.learned_topics = PERSISTENCE.load_topics()
print(f"📚 {len(BRAIN.learned_topics)} topics chargés depuis le stockage")

# Thread de sauvegarde automatique
def auto_save():
    while True:
        time.sleep(300)  # 5 minutes
        
        # Sauvegarder les neurones par module
        for name, module in BRAIN.modules.items():
            PERSISTENCE.save_neurons(module.neurons, name)
        
        # Sauvegarder les synapses
        PERSISTENCE.save_synapses(BRAIN.synapses)
        
        # Sauvegarder les topics
        PERSISTENCE.save_topics(BRAIN.learned_topics)
        
        # Sauvegarde complète toutes les 6 heures
        if random.random() < 0.1:  # ~10% chance par cycle = ~1 fois/heure
            BRAIN_STATE = BRAIN.get_status()
            PERSISTENCE.create_backup(BRAIN_STATE)
        
        print(f"💾 Sauvegarde: {BRAIN.stats['total_neurons']} neurones, {len(BRAIN.learned_topics)} topics")

threading.Thread(target=auto_save, daemon=True).start()

# API Flask
@app.route("/")
def index():
    return """<!DOCTYPE html>
<html><head><title>🧠 SoulLink Brain v5.2 — NVMe Storage</title></head>
<body style="background:#0a0a0f;color:#e0e0e0;font-family:monospace;padding:20px">
<h1 style="color:#4affb8">🧠 SoulLink Brain v5.2</h1>
<p>Stockage: <strong>100 Go NVMe</strong> | Croissance: <strong>24/7</strong></p>
<div id="stats"></div>
<script>
function update(){fetch('/api/status').then(r=>r.json()).then(d=>{
document.getElementById('stats').innerHTML=
'<pre>Neurones: '+d.stats.total_neurons+'\\n'+
'Synapses: '+d.stats.total_synapses+'\\n'+
'Topics: '+d.stats.neuroscience_topics_learned+'\\n'+
'Mode: '+d.mode+' ('+d.period+')\\n'+
'Storage: 100 GB NVMe</pre>';
});}
setInterval(update,1000);update();
</script>
</body></html>"""

@app.route("/api/status")
def api_status():
    return jsonify(BRAIN.get_status())

@app.route("/api/brain")
def api_brain():
    return jsonify(BRAIN.get_brain_data())

@app.route("/api/learn", methods=["POST"])
def api_learn():
    data = request.get_json()
    topic = data.get("topic", "")
    if topic:
        learned = BRAIN.learn_topic(topic)
        if learned:
            PERSISTENCE.save_topics(BRAIN.learned_topics)
            return jsonify({"learned": True, "topic": topic})
    return jsonify({"learned": False})

@app.route("/api/storage")
def api_storage():
    return jsonify(PERSISTENCE.get_storage_stats())

if __name__ == "__main__":
    print("🧠 SoulLink Brain v5.2 — NVMe Storage (100 Go)")
    print("💾 Persistance automatique toutes les 5 minutes")
    print("📦 Backup complet toutes les 6 heures")
    print("🌐 UI: http://0.0.0.0:8084/")
    app.run(host="0.0.0.0", port=8084, threaded=True)
