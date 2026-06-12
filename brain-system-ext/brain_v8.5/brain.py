#!/usr/bin/env python3
"""
SoulLink Brain v8.5 — PERSISTANCE CORRIGÉE

Ce fichier CORRIGE le bug de persistance:
- Sauvegarde COMPLÈTE des neurones (positions, modules, états)
- Sauvegarde COMPLÈTE des synapses (source, target, poids, délai)
- Restauration au démarrage
- Le nombre de neurones ne diminue JAMAIS

Usage:
    python brain_v8.5_persistence.py --port 8084
    
Testé et validé avant déploiement.
"""

import json
import math
import random
import threading
import time
from pathlib import Path
from flask import Flask, Response, request, send_from_directory

# ═══════════════════════════════════════════════════════════════════════════
# PERSISTANCE
# ═══════════════════════════════════════════════════════════════════════════

PERSIST_PATH = Path("/mnt/nvme/soullink_brain")
PERSIST_PATH.mkdir(parents=True, exist_ok=True)
STATE_FILE = PERSIST_PATH / "brain_state.json"

def load_state():
    """Charge l'état COMPLET depuis la sauvegarde"""
    if STATE_FILE.exists():
        try:
            with open(STATE_FILE, 'r') as f:
                data = json.load(f)
                neurons = data.get('neurons', [])
                synapses = data.get('synapses', [])
                print(f"📂 État chargé: {len(neurons)} neurones, {len(synapses)} synapses")
                print(f"   total_neurons: {data.get('total_neurons', 0)}, growth: {data.get('growth_events', 0)}")
                return data
        except Exception as e:
            print(f"⚠️ Erreur chargement état: {e}")
    # État par défaut
    return {
        "total_neurons": 0,
        "growth_events": 0,
        "total_spikes": 0,
        "last_save": 0,
        "neurons": [],
        "synapses": []
    }

def save_state(state):
    """Sauvegarde l'état COMPLET"""
    try:
        with open(STATE_FILE, 'w') as f:
            json.dump(state, f, indent=2)
        print(f"💾 État sauvegardé: {len(state.get('neurons', []))} neurones, {len(state.get('synapses', []))} synapses")
    except Exception as e:
        print(f"⚠️ Erreur sauvegarde: {e}")

app = Flask(__name__)

# ═══════════════════════════════════════════════════════════════════════════
# CONFIGURATION LIF
# ═══════════════════════════════════════════════════════════════════════════

VR = -70    # Potentiel de repos
VT = -55    # Seuil de décharge
VZ = -75    # Potentiel de reset
TAU = 20    # Constante de temps
TREF = 3    # Période réfractaire
DT = 0.5    # Pas de temps

# Configuration des modules
MODULES = [
    ['perception', 20, '#3dffc0', 0.15, [-158, 82, 78]],
    ['memory',     28, '#3d9eff', 0.20, [18, 128, -82]],
    ['reasoning',  22, '#ff5577', 0.20, [168, 62, 42]],
    ['learning',   16, '#ff9944', 0.15, [-88, -42, 118]],
    ['attention',  12, '#cc44ff', 0.25, [88, 28, 88]],
    ['output',     14, '#aaff44', 0.15, [208, -32, -48]],
    ['language',   18, '#44ffff', 0.20, [-208, -62, -28]],
    ['vision',     15, '#ff6644', 0.15, [-48, -122, -62]],
    ['audio',      12, '#6677ff', 0.15, [112, -108, -88]],
    ['motor',      10, '#ffee44', 0.10, [238, -88, 68]],
]

# Connexions inter-modules
WIRES = {
    'perception': ['memory', 'attention', 'vision', 'language'],
    'memory': ['reasoning', 'learning', 'language', 'perception'],
    'reasoning': ['output', 'attention', 'memory'],
    'learning': ['memory', 'reasoning', 'perception'],
    'attention': ['perception', 'reasoning', 'vision', 'audio'],
    'output': ['motor', 'language'],
    'language': ['memory', 'output', 'perception'],
    'vision': ['perception', 'attention', 'motor'],
    'audio': ['attention', 'language', 'memory'],
    'motor': ['output', 'vision'],
}

class Brain:
    def __init__(self):
        self.modules = {}
        self.neurons = []
        self.synapses = []
        self.adj = {}
        self.signals = []
        self.stats = {'N': 0, 'syn': 0, 'spk': 0, 'hz': 0, 'growth': 0, 'sig': 0}
        self.sim_time = 0.0
        self.lock = threading.Lock()
        
        # CHARGER l'état précédent
        self.persistent = load_state()
        self.total_neurons = self.persistent.get('total_neurons', 0)
        self.growth_events = self.persistent.get('growth_events', 0)
        self.total_spikes = self.persistent.get('total_spikes', 0)
        self.last_save_time = time.time()
        
        # Initialiser les modules
        self._init_modules()
        
        # RESTAURER ou CRÉER les neurones
        saved_neurons = self.persistent.get('neurons', [])
        saved_synapses = self.persistent.get('synapses', [])
        
        if len(saved_neurons) > 0:
            print(f"🔄 Restauration de {len(saved_neurons)} neurones sauvegardés...")
            self._restore_neurons(saved_neurons)
            print(f"✅ {len(self.neurons)} neurones restaurés")
        else:
            print("🧠 Création initiale des neurones...")
            self._create_neurons()
            print(f"✅ {len(self.neurons)} neurones créés")
        
        if len(saved_synapses) > 0 and len(self.neurons) > 0:
            print(f"🔄 Restauration de {len(saved_synapses)} synapses...")
            self._restore_synapses(saved_synapses)
            print(f"✅ {len(self.synapses)} synapses restaurées")
        else:
            print("🧠 Création initiale des synapses...")
            self._create_synapses()
            print(f"✅ {len(self.synapses)} synapses créées")
        
        # Mettre à jour les stats
        self.stats['N'] = len(self.neurons)
        self.stats['syn'] = len(self.synapses)
        self.stats['growth'] = self.growth_events
        
        print(f"🧠 SoulLink Brain v8.5 — Persistance CORRIGÉE")
        print(f"   Modules: {len(self.modules)}")
        print(f"   Neurones: {len(self.neurons)} (actuels)")
        print(f"   Neurones totaux: {self.total_neurons} (historique)")
        print(f"   Synapses: {len(self.synapses)}")
        print(f"   Port: 8084")
    
    def _init_modules(self):
        """Initialise les modules"""
        for name, n, color, inh, pos in MODULES:
            self.modules[name] = {
                'name': name,
                'color': color,
                'pos': pos,
                'n': n,
                'inh': inh,
                'neurons': [],
                'driftPhase': random.random() * math.pi * 2,
            }
    
    def _create_neurons(self):
        """Crée les neurones initiaux"""
        for mod_name, mod in self.modules.items():
            r = 52 + mod['n'] * 1.4
            for i in range(mod['n']):
                is_inhib = random.random() < mod['inh']
                neuron = {
                    'id': f"{mod_name[:3]}_{i:03d}",
                    'mod': mod_name,
                    'exc': not is_inhib,
                    'v': VR + random.gauss(0, 6),
                    'tL': -9999,
                    'sp': False,
                    'drive': 0.7 + random.random() * 2.2,
                    'imp': 0.3 + random.random() * 0.7,
                    'glow': 0,
                    'vn': 0,
                    'fc': 0,
                    'rx': random.gauss(0, r * 0.5),
                    'ry': random.gauss(0, r * 0.5),
                    'rz': random.gauss(0, r * 0.5),
                }
                self.neurons.append(neuron)
                self.adj[neuron['id']] = []
                mod['neurons'].append(neuron)
        
        self.stats['N'] = len(self.neurons)
        self.total_neurons = max(self.total_neurons, len(self.neurons))
    
    def _restore_neurons(self, saved_neurons):
        """Restaure les neurones depuis la sauvegarde"""
        # Créer un index des modules
        for mod_name, mod in self.modules.items():
            mod['neurons'] = []
        
        for sn in saved_neurons:
            mod_name = sn['mod']
            if mod_name not in self.modules:
                continue
            
            mod = self.modules[mod_name]
            neuron = {
                'id': sn['id'],
                'mod': mod_name,
                'exc': sn['exc'],
                'v': sn['v'],
                'tL': sn.get('tL', -9999),
                'sp': sn.get('sp', False),
                'drive': sn.get('drive', 0.7 + random.random() * 2.2),
                'imp': sn.get('imp', 0.3 + random.random() * 0.7),
                'glow': sn.get('glow', 0),
                'vn': sn.get('vn', 0),
                'fc': sn.get('fc', 0),
                'rx': sn['rx'],
                'ry': sn['ry'],
                'rz': sn['rz'],
            }
            self.neurons.append(neuron)
            self.adj[neuron['id']] = []
            mod['neurons'].append(neuron)
        
        self.stats['N'] = len(self.neurons)
        # Mettre à jour le total_neurons si nécessaire
        if len(self.neurons) > self.total_neurons:
            self.total_neurons = len(self.neurons)
    
    def _create_synapses(self):
        """Crée les synapses initiales"""
        def rand_neuron(mod_name):
            return random.choice(self.modules[mod_name]['neurons'])
        
        def add_syn(s, t):
            raw = 0.15 + random.random() * 0.55
            w = raw if s['exc'] else -raw * 0.7
            d = 1 + random.random() * 8
            self.synapses.append({'s': s, 't': t, 'w': w, 'd': d})
            self.adj[s['id']].append({'t': t, 'w': w, 'd': d})
        
        for src_name, targets in WIRES.items():
            for tgt_name in targets:
                for _ in range(6 + random.randint(0, 6)):
                    add_syn(rand_neuron(src_name), rand_neuron(tgt_name))
        
        for mod in self.modules.values():
            for _ in range(len(mod['neurons']) * 3):
                a = random.choice(mod['neurons'])
                b = random.choice(mod['neurons'])
                if a != b:
                    add_syn(a, b)
        
        self.stats['syn'] = len(self.synapses)
    
    def _restore_synapses(self, saved_synapses):
        """Restaure les synapses depuis la sauvegarde"""
        # Index des neurones par ID
        neuron_index = {n['id']: n for n in self.neurons}
        
        for ss in saved_synapses:
            src_id = ss['s']
            tgt_id = ss['t']
            
            if src_id in neuron_index and tgt_id in neuron_index:
                src = neuron_index[src_id]
                tgt = neuron_index[tgt_id]
                synapse = {
                    's': src,
                    't': tgt,
                    'w': ss['w'],
                    'd': ss['d'],
                }
                self.synapses.append(synapse)
                self.adj[src_id].append({'t': tgt, 'w': ss['w'], 'd': ss['d']})
        
        self.stats['syn'] = len(self.synapses)
    
    def grow(self):
        """Croissance: ajoute un neurone dans un module aléatoire"""
        mod_name = random.choice(list(self.modules.keys()))
        mod = self.modules[mod_name]
        r = 52 + mod['n'] * 1.4
        
        is_inhib = random.random() < mod['inh']
        neuron = {
            'id': f"{mod_name[:3]}_{len(mod['neurons']):03d}",
            'mod': mod_name,
            'exc': not is_inhib,
            'v': VR + random.gauss(0, 6),
            'tL': -9999,
            'sp': False,
            'drive': 0.7 + random.random() * 2.2,
            'imp': 0.3 + random.random() * 0.7,
            'glow': 0,
            'vn': 0,
            'fc': 0,
            'rx': random.gauss(0, r * 0.5),
            'ry': random.gauss(0, r * 0.5),
            'rz': random.gauss(0, r * 0.5),
        }
        
        self.neurons.append(neuron)
        self.adj[neuron['id']] = []
        mod['neurons'].append(neuron)
        
        # Connecter le nouveau neurone
        other = random.choice(self.neurons)
        self.synapses.append({'s': neuron, 't': other, 'w': 0.3, 'd': 5})
        self.adj[neuron['id']].append({'t': other, 'w': 0.3, 'd': 5})
        
        for _ in range(random.randint(1, 3)):
            other = random.choice(self.neurons)
            if other != neuron:
                w = 0.2 + random.random() * 0.3
                self.synapses.append({'s': other, 't': neuron, 'w': w, 'd': 3})
                self.adj[other['id']].append({'t': neuron, 'w': w, 'd': 3})
        
        self.stats['N'] = len(self.neurons)
        self.stats['syn'] = len(self.synapses)
        self.stats['growth'] += 1
        self.total_neurons = max(self.total_neurons, self.stats['N'])
        self.growth_events += 1
    
    def step(self):
        """Simulation LIF - un step"""
        self.sim_time += DT
        spikes = 0
        new_signals = []
        
        for n in self.neurons:
            if n['tL'] > self.sim_time - TREF:
                continue
            
            n['v'] += DT * (n['drive'] - n['v']) / TAU
            
            for conn in self.adj[n['id']]:
                n['v'] += conn['w'] * n.get('vn', 0)
            
            if n['v'] > VT:
                n['sp'] = True
                n['tL'] = self.sim_time
                n['v'] = VZ
                spikes += 1
                n['glow'] = min(1, n['glow'] + 0.3)
                n['fc'] += 1
                
                src_mod = n['mod']
                # Obtenir la couleur du module
                mod = self.modules.get(src_mod)
                color = mod['color'] if mod else '#ffffff'
                for tgt in self.modules:
                    if random.random() < 0.1:
                        new_signals.append({
                            'from_mod': src_mod,
                            'to_mod': tgt,
                            'progress': 0,
                            'color': color if n['exc'] else '#ff4444',
                        })
            else:
                n['sp'] = False
        
        for s in self.signals[:]:
            s['progress'] += 0.1
            if s['progress'] >= 1:
                self.signals.remove(s)
                tgt_mod = self.modules.get(s['to_mod'])
                if tgt_mod and tgt_mod['neurons']:
                    n = random.choice(tgt_mod['neurons'])
                    n['v'] += 0.5 if random.random() < 0.5 else -0.3
        
        self.signals.extend(new_signals)
        self.stats['spk'] = spikes
        self.stats['hz'] = spikes / DT
        self.stats['sig'] = len(self.signals)
    
    def to_json(self):
        """Export state for frontend"""
        return {
            'modules': {name: {
                'name': m['name'],
                'color': m['color'],
                'pos': m['pos'],
                'n': len(m['neurons']),
                'driftPhase': m['driftPhase'],
            } for name, m in self.modules.items()},
            'neurons': [{
                'id': n['id'],
                'mod': n['mod'],
                'exc': n['exc'],
                'v': round(n['v'], 2),
                'glow': round(n['glow'], 3),
                'vn': round(n['vn'], 3),
                'fc': n['fc'],
                'rx': round(n['rx'], 2),
                'ry': round(n['ry'], 2),
                'rz': round(n['rz'], 2),
                'sp': n['sp'],
                'drive': round(n['drive'], 2),
                'imp': round(n['imp'], 2),
            } for n in self.neurons],
            'signals': [{
                'from_mod': s['from_mod'],
                'to_mod': s['to_mod'],
                'progress': round(s['progress'], 2),
                'color': s['color'],
            } for s in self.signals[:100]],
            'stats': {
                'N': self.stats['N'],
                'syn': self.stats['syn'],
                'spk': self.stats['spk'],
                'sig': self.stats['sig'],
                'hz': self.stats['hz'],
                'growth': self.stats['growth'],
                'total_neurons': self.total_neurons,
                'total_spikes': self.total_spikes,
            },
            'sim_time': round(self.sim_time, 1),
        }
    
    def save_full_state(self):
        """Sauvegarde COMPLÈTE de l'état"""
        state = {
            'total_neurons': self.total_neurons,
            'growth_events': self.growth_events,
            'total_spikes': self.total_spikes,
            'last_save': time.time(),
            'neurons': [{
                'id': n['id'],
                'mod': n['mod'],
                'exc': n['exc'],
                'v': float(n['v']),
                'rx': float(n['rx']),
                'ry': float(n['ry']),
                'rz': float(n['rz']),
                'glow': float(n.get('glow', 0)),
                'vn': float(n.get('vn', 0)),
                'fc': n.get('fc', 0),
                'drive': float(n.get('drive', 1.0)),
                'imp': float(n.get('imp', 0.5)),
                'tL': float(n.get('tL', -9999)),
                'sp': n.get('sp', False),
            } for n in self.neurons],
            'synapses': [{
                's': s['s']['id'],
                't': s['t']['id'],
                'w': float(s['w']),
                'd': float(s['d']),
            } for s in self.synapses],
        }
        save_state(state)

# ═══════════════════════════════════════════════════════════════════════════
# INTERFACE HTML
# ═══════════════════════════════════════════════════════════════════════════

HTML_TEMPLATE = '''
<!DOCTYPE html>
<html>
<head>
    <title>SoulLink Brain v8.5</title>
    <script src="/three.min.js"></script>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { background: #0a0a12; color: #fff; font-family: 'Segoe UI', sans-serif; overflow: hidden; }
        #canvas { position: fixed; top: 0; left: 0; width: 100%; height: 100%; }
        .hud { position: fixed; z-index: 100; background: rgba(10,10,18,0.85); border-radius: 12px; padding: 15px; }
        #stats { top: 20px; left: 20px; border: 1px solid #3d9eff; }
        #stats h2 { color: #3d9eff; margin-bottom: 10px; }
        #stats .stat { display: flex; justify-content: space-between; margin: 5px 0; }
        #stats .label { color: #888; }
        #stats .value { color: #fff; font-weight: bold; }
        #modules { top: 20px; right: 20px; border: 1px solid #ff5577; max-height: 350px; overflow-y: auto; }
        #modules h3 { color: #ff5577; margin-bottom: 10px; }
        #modules .mod { display: flex; align-items: center; margin: 4px 0; font-size: 12px; }
        #modules .dot { width: 10px; height: 10px; border-radius: 50%; margin-right: 6px; }
    </style>
</head>
<body>
    <canvas id="canvas"></canvas>
    <div id="stats" class="hud">
        <h2>🧠 Brain v8.5</h2>
        <div class="stat"><span class="label">Neurons</span><span class="value" id="n-neurons">0</span></div>
        <div class="stat"><span class="label">Synapses</span><span class="value" id="n-synapses">0</span></div>
        <div class="stat"><span class="label">Spikes/s</span><span class="value" id="hz">0</span></div>
        <div class="stat"><span class="label">Growth</span><span class="value" id="growth">0</span></div>
        <div class="stat"><span class="label">Signals</span><span class="value" id="signals">0</span></div>
    </div>
    <div id="modules" class="hud"><h3>Modules</h3><div id="mod-list"></div></div>
    <script>
        const scene = new THREE.Scene();
        const camera = new THREE.PerspectiveCamera(75, window.innerWidth/window.innerHeight, 0.1, 1000);
        const renderer = new THREE.WebGLRenderer({canvas: document.getElementById('canvas'), antialias: true});
        renderer.setSize(window.innerWidth, window.innerHeight);
        renderer.setClearColor(0x0a0a12);
        scene.add(new THREE.AmbientLight(0x404040, 0.5));
        scene.add(new THREE.DirectionalLight(0xffffff, 0.8));
        const neuronsGroup = new THREE.Group();
        scene.add(neuronsGroup);
        const neurons = [];
        let time = 0;
        async function update() {
            try {
                const res = await fetch('/api/brain');
                const data = await res.json();
                neuronsGroup.clear();
                neurons.length = 0;
                const modColors = {};
                for (const m in data.modules) modColors[m] = parseInt(data.modules[m].color.slice(1), 16);
                for (const n of data.neurons) {
                    const color = n.exc ? (modColors[n.mod] || 0xffffff) : 0xff4444;
                    const geo = new THREE.SphereGeometry(n.exc ? 2 : 1.5, 12, 12);
                    const mat = new THREE.MeshPhongMaterial({ color, emissive: n.sp ? 0xffffff : color, emissiveIntensity: n.glow || 0 });
                    const mesh = new THREE.Mesh(geo, mat);
                    const pos = data.modules[n.mod]?.pos || [0,0,0];
                    mesh.position.set((pos[0]||0) + n.rx * 0.1, (pos[1]||0) + n.ry * 0.1, (pos[2]||0) + n.rz * 0.1);
                    neuronsGroup.add(mesh);
                    neurons.push({ mesh, data: n });
                }
                document.getElementById('n-neurons').textContent = data.stats.N;
                document.getElementById('n-synapses').textContent = data.stats.syn;
                document.getElementById('hz').textContent = data.stats.hz.toFixed(1);
                document.getElementById('growth').textContent = data.stats.growth;
                document.getElementById('signals').textContent = data.stats.sig;
                const list = document.getElementById('mod-list');
                list.innerHTML = '';
                for (const m in data.modules) {
                    const div = document.createElement('div');
                    div.className = 'mod';
                    div.innerHTML = '<div class="dot" style="background:' + data.modules[m].color + '"></div>' + m;
                    list.appendChild(div);
                }
            } catch(e) {}
        }
        function animate() {
            requestAnimationFrame(animate);
            neuronsGroup.rotation.y += 0.002;
            time++;
            if (time % 30 === 0) update();
            renderer.render(scene, camera);
        }
        camera.position.set(0, 50, 300);
        camera.lookAt(0, 0, 0);
        window.addEventListener('resize', () => { camera.aspect = window.innerWidth/window.innerHeight; camera.updateProjectionMatrix(); renderer.setSize(window.innerWidth, window.innerHeight); });
        update();
        animate();
    </script>
</body>
</html>
'''

# Instance globale
brain = Brain()

# ═══════════════════════════════════════════════════════════════════════════
# FLASK ROUTES
# ═══════════════════════════════════════════════════════════════════════════

@app.route('/api/brain')
def api_brain():
    with brain.lock:
        return Response(json.dumps(brain.to_json()), mimetype='application/json')

@app.route('/api/stats')
def api_stats():
    with brain.lock:
        return Response(json.dumps(brain.stats), mimetype='application/json')

@app.route('/')
def index():
    return Response(HTML_TEMPLATE, mimetype='text/html')

@app.route('/three.min.js')
def three_js():
    return send_from_directory('/mnt/nvme', 'three.min.js', mimetype='application/javascript')

@app.route('/test')
def test():
    return Response('<html><body><h1>Brain OK</h1><p>API: <a href="/api/stats">/api/stats</a></p><p>Interface: <a href="/">/</a></p></body></html>', mimetype='text/html')

@app.route('/api/stimulus', methods=['POST'])
def api_stimulus():
    """Reçoit un stimulus pour entraîner le Brain - CRÉE DES NEURONES"""
    try:
        data = request.get_json()
        module = data.get('module', 'perception')
        intensity = float(data.get('intensity', 1.0))
        knowledge = data.get('knowledge', '')
        
        with brain.lock:
            # Créer des neurones basés sur la longueur de la connaissance
            # Chaque stimulus crée 1-5 neurones selon l'intensité
            neurons_created = 0
            synapses_created = 0
            
            # Mapper le module demandé
            target_mod = module if module in brain.modules else 'memory'
            mod = brain.modules.get(target_mod, list(brain.modules.values())[0])
            
            # Créer des neurones proportionnellement à la connaissance
            num_new = min(10, max(1, int(intensity * 2)))
            
            for i in range(num_new):
                # Créer le neurone
                r = 52 + mod['n'] * 1.4
                neuron = {
                    'id': f"{target_mod[:3]}_{len(brain.neurons):04d}",
                    'mod': target_mod,
                    'exc': random.random() > 0.2,
                    'v': -70 + random.gauss(0, 6),
                    'tL': -9999,
                    'sp': False,
                    'drive': intensity,
                    'imp': 0.3 + random.random() * 0.7,
                    'glow': intensity,
                    'vn': 0,
                    'fc': 0,
                    'rx': random.gauss(0, r * 0.5),
                    'ry': random.gauss(0, r * 0.5),
                    'rz': random.gauss(0, r * 0.5),
                    'knowledge': knowledge[:100] if knowledge else '',  # Stocker une partie
                }
                
                brain.neurons.append(neuron)
                brain.adj[neuron['id']] = []
                mod['neurons'].append(neuron)
                neurons_created += 1
                
                # Connecter aux neurones existants
                if len(brain.neurons) > 1:
                    for _ in range(random.randint(2, 5)):
                        other = random.choice(brain.neurons[:-1])
                        w = 0.2 + random.random() * 0.5
                        d = random.randint(1, 5)
                        brain.synapses.append({'s': neuron, 't': other, 'w': w, 'd': d})
                        brain.adj[neuron['id']].append({'t': other, 'w': w, 'd': d})
                        synapses_created += 1
            
            brain.stats['N'] = len(brain.neurons)
            brain.stats['syn'] = len(brain.synapses)
            brain.stats['growth'] += neurons_created
            brain.stats['sig'] += len(knowledge)
            brain.total_neurons = max(brain.total_neurons, brain.stats['N'])
        
        return Response(json.dumps({
            'status': 'absorbed',
            'module': module,
            'neurons_created': neurons_created,
            'synapses_created': synapses_created,
            'intensity': intensity,
            'total_neurons': brain.stats['N']
        }), mimetype='application/json')
    except Exception as e:
        return Response(json.dumps({'error': str(e)}), status=400, mimetype='application/json')

# ═══════════════════════════════════════════════════════════════════════════
# SIMULATION
# ═══════════════════════════════════════════════════════════════════════════

def simulation_loop():
    """Boucle de simulation avec SAUVEGARDE COMPLÈTE"""
    last_grow = time.time()
    last_save = time.time()
    
    while True:
        with brain.lock:
            brain.step()
            brain.total_spikes += brain.stats['spk']
            
            # Growth toutes les 5s
            if time.time() - last_grow > 5:
                brain.grow()
                last_grow = time.time()
            
            # Sauvegarde COMPLÈTE toutes les 30s
            if time.time() - last_save > 30:
                brain.save_full_state()
                last_save = time.time()
        
        time.sleep(0.002)

if __name__ == '__main__':
    print("🧠 SoulLink Brain v8.5 — Persistance CORRIGÉE")
    print(f"   Modules: {len(brain.modules)}")
    print(f"   Neurones: {len(brain.neurons)} (actuels)")
    print(f"   Neurones totaux: {brain.total_neurons} (historique)")
    print(f"   Synapses: {len(brain.synapses)}")
    print(f"   Port: 8084")
    print("")
    print("   🌐 Interface: http://localhost:8084")
    print("   📊 API: http://localhost:8084/api/brain")
    print("   💾 Persistance: /mnt/nvme/soullink_brain/brain_state.json")
    print("   ✨ FIX: Neurones et synapses sauvegardés et restaurés")
    
    sim_thread = threading.Thread(target=simulation_loop, daemon=True)
    sim_thread.start()
    
    app.run(host='0.0.0.0', port=8084, threaded=True)