import sys
sys.path.insert(0, "/mnt/nvme/soullink_brain")
from brain_persistence import PERSISTENCE
#!/usr/bin/env python3
"""
SoulLink Brain v5.1 — CROISSANCE ACCÉLÉRÉE + VISUALISATION NEURONALE
- Visualisation graphique avec bulles, spikes, connexions
- Consolidation qui RENFORCE (ne détruit pas)
- Croissance continue 24/7
"""
import json
import random
import threading
import time
import math
from datetime import datetime
from collections import deque
from flask import Flask, jsonify, request

app = Flask("brain_viz")

# ═══════════════════════════════════════════════════════════════════════════
# NEURONE INDIVIDUEL
# ═══════════════════════════════════════════════════════════════════════════

class Neuron:
    """Neurone individuel avec visualisation."""
    
    def __init__(self, neuron_id: str, layer: str):
        self.id = neuron_id
        self.layer = layer
        self.potential = random.uniform(0.3, 0.7)
        self.activation = 0.0
        self.firing_threshold = 0.75 + random.uniform(-0.1, 0.1)
        self.connections_out = []
        self.connections_in = []
        self.last_fired = 0
        self.firing_count = 0
        self.strength = random.uniform(0.5, 1.0)
        self.learning_rate = random.uniform(0.1, 0.3)
        self.consolidation_strength = 1.0
        self.importance = random.uniform(0.3, 0.7)
        # Pour visualisation
        self.x = random.uniform(50, 750)
        self.y = random.uniform(50, 450)
        self.spike_time = 0  # Pour animation spike
    
    def update(self, dt: float, external_input: float = 0.0):
        """Met à jour le potentiel."""
        synaptic_input = sum(s.weight * s.source.activation for s in self.connections_in if s.source)
        total_input = external_input + synaptic_input
        self.potential = self.potential * 0.98 + total_input * 0.02
        self.activation = 1.0 / (1.0 + math.exp(-10 * (self.potential - 0.5)))
        
        if self.potential > self.firing_threshold:
            self.fire()
        return self.activation
    
    def fire(self):
        """Déclenche le neurone."""
        self.potential = 0.0
        self.last_fired = time.time()
        self.firing_count += 1
        self.spike_time = time.time()  # Pour animation
        self.importance = min(1.0, self.importance + 0.03)
    
    def consolidate(self, strength: float = 1.0):
        """Consolide (RENFORCE)."""
        self.consolidation_strength += strength * 0.1
        self.importance = min(1.0, self.importance + 0.02)
        for synapse in self.connections_out:
            if synapse.target and synapse.target.importance > 0.5:
                synapse.weight = min(1.0, synapse.weight + 0.05)

class Synapse:
    """Connexion entre neurones."""
    
    def __init__(self, source: Neuron, target: Neuron):
        self.source = source
        self.target = target
        self.weight = random.uniform(0.3, 0.7)
        self.hebbian_strength = 1.0
    
    def strengthen(self, delta: float = 0.01):
        self.weight = min(1.0, self.weight + delta)
        self.hebbian_strength += delta

# ═══════════════════════════════════════════════════════════════════════════
# MODULE CÉRÉBRAL
# ═══════════════════════════════════════════════════════════════════════════

class BrainModule:
    """Module cérébral avec position pour visualisation."""
    
    def __init__(self, name: str, initial_neurons: int = 10, x: float = 400, y: float = 250, color: str = "#4a9eff"):
        self.name = name
        self.neurons = []
        self.activation_level = random.uniform(0.5, 0.8)
        self.importance = random.uniform(0.3, 0.7)
        self.x = x
        self.y = y
        self.color = color
        
        for i in range(initial_neurons):
            self.add_neuron()
    
    def add_neuron(self) -> Neuron:
        neuron_id = f"{self.name[:3]}_{len(self.neurons):03d}"
        neuron = Neuron(neuron_id, self.name)
        # Position relative au module
        neuron.x = self.x + random.uniform(-60, 60)
        neuron.y = self.y + random.uniform(-40, 40)
        self.neurons.append(neuron)
        return neuron
    
    def update(self, dt: float, external_input: float = 0.0):
        total_activation = 0
        for neuron in self.neurons:
            activation = neuron.update(dt, external_input * self.activation_level)
            total_activation += activation
        self.activation_level = total_activation / max(1, len(self.neurons))
        return self.activation_level

# ═══════════════════════════════════════════════════════════════════════════
# CERVEAU COMPLET
# ═══════════════════════════════════════════════════════════════════════════

class VisualBrain:
    """Cerveau avec visualisation et croissance accélérée."""
    
    def __init__(self):
        self.modules = {}
        self.synapses = []
        self.spikes = []  # Pour animation
        
        # Périodes avec croissance CONTINUE
        self.periods = [
            {"name": "deep_night", "start": 0, "end": 5, "intensity": 0.4, "mode": "consolidate"},
            {"name": "dawn", "start": 5, "end": 7, "intensity": 0.6, "mode": "consolidate"},
            {"name": "morning_peak", "start": 7, "end": 12, "intensity": 1.0, "mode": "grow"},
            {"name": "midday_rest", "start": 12, "end": 14, "intensity": 0.7, "mode": "maintain"},
            {"name": "afternoon_peak", "start": 14, "end": 18, "intensity": 0.95, "mode": "grow"},
            {"name": "evening", "start": 18, "end": 22, "intensity": 0.8, "mode": "maintain"},
            {"name": "night_fall", "start": 22, "end": 24, "intensity": 0.5, "mode": "consolidate"},
        ]
        
        self.stats = {
            "total_neurons": 0, "total_synapses": 0, "firing_neurons": 0,
            "growth_events": 0, "consolidation_events": 0, "learning_cycles": 0,
            "neuroscience_topics_learned": 0, "spikes_count": 0,
        }
        
        self.learning_queue = deque(maxlen=1000)
        self.learned_topics = set()
        
        self._init_modules()
        self.running = True
        self.growth_thread = threading.Thread(target=self._growth_loop, daemon=True)
        self.growth_thread.start()
    
    def _init_modules(self):
        """Initialise les modules avec positions."""
        configs = [
            ("perception", 12, 150, 100, "#4affb8"),
            ("memory", 18, 350, 80, "#4a9eff"),
            ("reasoning", 15, 550, 100, "#ff4a6a"),
            ("learning", 10, 200, 220, "#ffaa4a"),
            ("output", 8, 400, 220, "#9bff4a"),
            ("attention", 6, 600, 220, "#ff4aff"),
            ("language", 10, 150, 350, "#4affff"),
            ("vision", 8, 350, 350, "#ff4a4a"),
            ("audio", 6, 550, 350, "#4a4aff"),
            ("motor", 5, 400, 420, "#ffff4a"),
        ]
        
        for name, neurons, x, y, color in configs:
            self.modules[name] = BrainModule(name, neurons, x, y, color)
        
        self._create_connections()
        self._update_stats()
    
    def _create_connections(self):
        """Crée des connexions synaptiques."""
        module_names = list(self.modules.keys())
        
        for i, mod_name in enumerate(module_names):
            source_module = self.modules[mod_name]
            
            # Connexions vers modules adjacents
            for j in [-1, 0, 1]:
                target_idx = (i + j) % len(module_names)
                target_module = self.modules[module_names[target_idx]]
                
                for _ in range(min(3, len(source_module.neurons))):
                    if source_module.neurons and target_module.neurons:
                        source = random.choice(source_module.neurons)
                        target = random.choice(target_module.neurons)
                        
                        synapse = Synapse(source, target)
                        source.connections_out.append(synapse)
                        target.connections_in.append(synapse)
                        self.synapses.append(synapse)
    
    def get_current_period(self):
        """Retourne la période actuelle."""
        hour = datetime.now().hour
        for period in self.periods:
            start, end = period["start"], period["end"]
            if start <= hour < end:
                return period
        return self.periods[0]
    
    def _growth_loop(self):
        """Boucle de croissance continue."""
        while self.running:
            period = self.get_current_period()
            intensity = period["intensity"]
            mode = period["mode"]
            
            if mode == "consolidate":
                self._consolidate(intensity)
            elif mode == "grow":
                self._grow(intensity * 1.5)
            else:
                self._grow(intensity)
            
            self._learn_continuously()
            time.sleep(2)
    
    def _consolidate(self, intensity: float):
        """CONSOLIDE (RENFORCE, pas destruction)."""
        consolidation_events = 0
        
        for module in self.modules.values():
            for neuron in module.neurons:
                if neuron.importance > 0.4:
                    neuron.consolidate(intensity)
                    consolidation_events += 1
                
                for synapse in neuron.connections_out:
                    if synapse.source.firing_count > 0 and synapse.target.firing_count > 0:
                        synapse.strengthen(0.02 * intensity)
        
        self.stats["consolidation_events"] += consolidation_events
        
        # Croissance même pendant consolidation
        if random.random() < 0.3 * intensity:
            self._add_neuron_to_important_module()
    
    def _grow(self, intensity: float):
        """Croissance de nouveaux neurones."""
        new_neurons = int(intensity * 2) + random.randint(0, 2)
        
        for _ in range(new_neurons):
            self._add_neuron_to_important_module()
        
        new_synapses = int(intensity * 3)
        for _ in range(new_synapses):
            self._create_random_synapse()
        
        self.stats["growth_events"] += new_neurons
    
    def _add_neuron_to_important_module(self):
        """Ajoute un neurone à un module important."""
        weighted = [(m, m.importance) for m in self.modules.values()]
        total = sum(w for _, w in weighted)
        
        if total > 0:
            r = random.uniform(0, total)
            cumulative = 0
            for module, importance in weighted:
                cumulative += importance
                if r <= cumulative:
                    module.add_neuron()
                    module.importance = min(1.0, module.importance + 0.01)
                    break
        else:
            random.choice(list(self.modules.values())).add_neuron()
    
    def _create_random_synapse(self):
        """Crée une synapse aléatoire."""
        modules = list(self.modules.values())
        if len(modules) < 2:
            return
        
        source_module = random.choice(modules)
        target_module = random.choice([m for m in modules if m != source_module] or modules)
        
        if source_module.neurons and target_module.neurons:
            source = random.choice(source_module.neurons)
            target = random.choice(target_module.neurons)
            
            synapse = Synapse(source, target)
            source.connections_out.append(synapse)
            target.connections_in.append(synapse)
            self.synapses.append(synapse)
    
    def _learn_continuously(self):
        """Apprentissage continu."""
        self.stats["learning_cycles"] += 1
        
        # Mettre à jour les neurones
        firing_now = 0
        spikes_now = []
        
        for module in self.modules.values():
            module.update(0.1, random.uniform(0.1, 0.5))
            
            for neuron in module.neurons:
                if neuron.potential > neuron.firing_threshold:
                    firing_now += 1
                    # Enregistrer le spike pour visualisation
                    spikes_now.append({
                        "id": neuron.id,
                        "x": neuron.x,
                        "y": neuron.y,
                        "time": neuron.spike_time
                    })
        
        self.spikes = spikes_now[-50:]  # Garder les 50 derniers spikes
        self.stats["spikes_count"] = len(self.spikes)
        self.stats["firing_neurons"] = firing_now
        self._update_stats()
    
    def learn_topic(self, topic: str):
        """Apprend un nouveau topic."""
        if topic not in self.learned_topics:
            for _ in range(random.randint(2, 5)):
                self._add_neuron_to_important_module()
            
            for _ in range(random.randint(3, 8)):
                self._create_random_synapse()
            
            self.learned_topics.add(topic)
            self.stats["neuroscience_topics_learned"] += 1
            self._update_stats()
            return True
        return False
    
    def _update_stats(self):
        """Met à jour les statistiques."""
        total_neurons = sum(len(m.neurons) for m in self.modules.values())
        firing = sum(1 for m in self.modules.values() for n in m.neurons if n.firing_count > 0)
        
        self.stats["total_neurons"] = total_neurons
        self.stats["total_synapses"] = len(self.synapses)
        self.stats["firing_neurons"] = firing
    
    def get_status(self):
        """Retourne le statut pour l'API."""
        period = self.get_current_period()
        
        modules_status = {}
        for name, module in self.modules.items():
            modules_status[name] = {
                "neurons": len(module.neurons),
                "activation": round(module.activation_level, 3),
                "importance": round(module.importance, 3),
                "x": module.x,
                "y": module.y,
                "color": module.color,
            }
        
        return {
            "stats": self.stats,
            "period": period["name"],
            "intensity": period["intensity"],
            "mode": period["mode"],
            "modules": modules_status,
            "timestamp": datetime.now().isoformat(),
        }
    
    def get_brain_data(self):
        """Retourne les données pour visualisation."""
        neurons = []
        for module in self.modules.values():
            for neuron in module.neurons:
                # Vérifier si spike récent
                is_spiking = time.time() - neuron.spike_time < 0.5 if neuron.spike_time else False
                
                neurons.append({
                    "id": neuron.id,
                    "layer": neuron.layer,
                    "x": neuron.x,
                    "y": neuron.y,
                    "potential": round(neuron.potential, 3),
                    "activation": round(neuron.activation, 3),
                    "importance": round(neuron.importance, 3),
                    "firing_count": neuron.firing_count,
                    "is_spiking": is_spiking,
                })
        
        synapses = []
        for synapse in self.synapses:
            if synapse.source and synapse.target:
                synapses.append({
                    "source": synapse.source.id,
                    "target": synapse.target.id,
                    "weight": round(synapse.weight, 3),
                })
        
        spikes = [s for s in self.spikes if time.time() - s["time"] < 0.3]
        
        return {
            "neurons": neurons,
            "synapses": synapses,
            "spikes": spikes,
            "modules": {name: {"x": m.x, "y": m.y, "color": m.color, "neurons": len(m.neurons)} 
                       for name, m in self.modules.items()},
        }

# Instance globale
BRAIN = VisualBrain()

# ═══════════════════════════════════════════════════════════════════════════
# HTML AVEC VISUALISATION CYTOSCAPE
# ═══════════════════════════════════════════════════════════════════════════

HTML = """<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>Brain v5.1 - Visualisation Neuronale</title>
<script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.26.0/cytoscape.min.js"></script>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:JetBrains Mono,monospace;background:#0a0a0f;color:#e0e0e0;overflow:hidden}
#header{padding:15px;background:linear-gradient(90deg,#1a1a2e,#16213e);border-bottom:1px solid #333}
#header h1{font-size:16px;color:#4affb8}
#header .stats{display:flex;gap:20px;margin-top:8px;font-size:12px}
#header .stat .lbl{color:#888}
#header .stat .val{color:#4affb8;font-weight:700}
#cy{width:800px;height:500px;float:left;background:#0a0a0f}
#panel{float:right;width:calc(100% - 820px);height:500px;padding:10px;overflow-y:auto}
#panel h2{font-size:14px;color:#4a9eff;margin-bottom:10px}
.module-card{background:#1a1a24;padding:10px;margin-bottom:8px;border-radius:6px;border-left:3px solid #4a9eff}
.module-card .name{font-size:12px;color:#888}
.module-card .count{font-size:20px;color:#4affb8;font-weight:700}
.module-card .bar{height:4px;background:#333;margin-top:5px;border-radius:2px}
.module-card .bar .fill{height:100%;border-radius:2px}
#period{padding:10px;background:#111;margin:10px;border-radius:4px}
#period.grow{border-left:4px solid #4affb8}
#period.consolidate{border-left:4px solid #ffaa4a}
#period.maintain{border-left:4px solid #4a9eff}
#learn{margin:10px}
#learn input{background:#111;border:1px solid #333;color:#e0e0e0;padding:8px;width:200px;border-radius:4px}
#learn button{background:#4a9eff;border:none;color:#000;padding:8px 15px;border-radius:4px;cursor:pointer;font-weight:700}
</style></head>
<body>
<div id="header">
<h1>🧠 SoulLink Brain v5.1 — Visualisation Neuronale</h1>
<div class="stats">
<div class="stat"><span class="lbl">Neurones</span> <span class="val" id="neurons">0</span></div>
<div class="stat"><span class="lbl">Synapses</span> <span class="val" id="synapses">0</span></div>
<div class="stat"><span class="lbl">Spikes</span> <span class="val" id="spikes">0</span></div>
<div class="stat"><span class="lbl">Topics</span> <span class="val" id="topics">0</span></div>
<div class="stat"><span class="lbl">Growth</span> <span class="val" id="growth">0</span></div>
</div>
</div>
<div id="period"></div>
<div id="cy"></div>
<div id="panel">
<h2>MODULES</h2>
<div id="modules"></div>
<div id="learn">
<input type="text" id="topic" placeholder="Topic à apprendre...">
<button onclick="learnTopic()">Apprendre</button>
</div>
</div>
<script>
let cy;
function initCytoscape(data){
cy = cytoscape({
container: document.getElementById('cy'),
elements: [],
style: [
{selector: 'node', style: {'background-color': 'data(color)', 'width': 12, 'height': 12, 'label': 'data(label)', 'font-size': 6, 'color': '#888'}},
{selector: 'edge', style: {'width': 1, 'line-color': '#333', 'opacity': 0.3}},
{selector: '.spiking', style: {'background-color': '#fff', 'width': 18, 'height': 18, 'border-width': 3, 'border-color': '#4affb8', 'border-opacity': 1}},
{selector: '.module', style: {'background-color': 'data(color)', 'width': 40, 'height': 40, 'label': 'data(label)', 'font-size': 10, 'color': '#fff', 'text-valign': 'center', 'text-halign': 'center'}}
],
layout: {name: 'preset'}
});
updateGraph(data);
}
function updateGraph(data){
if(!cy) return;
// Modules comme gros nœuds
let nodes = Object.entries(data.modules).map(([name, m]) => ({
data: {id: 'mod_' + name, label: name.toUpperCase(), color: m.color, type: 'module'}, classes: 'module', position: {x: m.x, y: m.y}
}));
// Neurones
data.neurons.forEach(n => {
nodes.push({data: {id: n.id, label: '', color: n.is_spiking ? '#fff' : getNeuronColor(n), type: 'neuron'}, 
classes: n.is_spiking ? 'spiking' : '', position: {x: n.x, y: n.y}});
});
// Synapses
let edges = data.synapses.slice(0, 500).map(s => ({data: {id: s.source + '_' + s.target, source: s.source, target: s.target}}));
cy.json({elements: {nodes: nodes, edges: edges}});
}
function getNeuronColor(n){
if(n.is_spiking) return '#fff';
if(n.importance > 0.7) return '#4affb8';
if(n.importance > 0.5) return '#4a9eff';
return '#666';
}
function fetchBrain(){
fetch('/api/brain').then(r => r.json()).then(d => {
document.getElementById('neurons').textContent = d.neurons.length;
document.getElementById('synapses').textContent = d.synapses.length;
document.getElementById('spikes').textContent = d.spikes.length;
updateGraph(d);
});
}
function fetchStatus(){
fetch('/api/status').then(r => r.json()).then(d => {
document.getElementById('topics').textContent = d.stats.neuroscience_topics_learned;
document.getElementById('growth').textContent = d.stats.growth_events;
const p = document.getElementById('period');
p.className = d.mode;
p.innerHTML = '<strong>' + d.period + '</strong> | Intensité: ' + (d.intensity * 100).toFixed(0) + '% | Mode: ' + d.mode.toUpperCase();
let html = '';
for(const [name, m] of Object.entries(d.modules)){
html += '<div class="module-card" style="border-color:' + m.color + '">' +
'<div class="name">' + name.toUpperCase() + '</div>' +
'<div class="count">' + m.neurons + '</div>' +
'<div class="bar"><div class="fill" style="width:' + (m.activation * 100) + '%;background:' + m.color + '"></div></div>' +
'</div>';
}
document.getElementById('modules').innerHTML = html;
});
}
function learnTopic(){
const topic = document.getElementById('topic').value;
if(!topic) return;
fetch('/api/learn', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({topic: topic})})
.then(r => r.json()).then(d => { document.getElementById('topic').value = ''; fetchStatus(); });
}
// Initialisation
fetch('/api/brain').then(r => r.json()).then(initCytoscape);
setInterval(fetchBrain, 1000);
setInterval(fetchStatus, 1000);
</script>
</body></html>
"""

@app.route("/")
def index():
    return HTML

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
            return jsonify({"learned": True, "topic": topic, "new_neurons": random.randint(2, 5)})
    return jsonify({"learned": False})

if __name__ == "__main__":
    print("🧠 SoulLink Brain v5.1 — Visualisation Neuronale")
    print("📊 Consolidation qui RENFORCE (ne détruit pas)")
    print("🌐 UI: http://0.0.0.0:8084/")
    app.run(host="0.0.0.0", port=8084, threaded=True)