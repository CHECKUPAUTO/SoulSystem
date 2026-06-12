#!/usr/bin/env python3
"""
SoulLink Brain v8.4 — 3D Bubbles ULTIME + PERSISTANCE
Port 8084 — Simulation LIF + Signaux Animés + Particules

Features:
- 10 modules cérébraux avec bulles 3D wireframe
- Neurones animés à l'intérieur des bulles (spike, glow, normal)
- SIGNAUX ANIMÉS entre modules (particules voyageant sur les connexions)
- HUD temps réel (N, spikes, signaux, synapses, Hz, growth)
- Interactions: Drag, Scroll, Click sur bulles/neurones
- Auto-grow: ajout automatique de neurones toutes les 5s
- Simulation LIF (Leaky Integrate-and-Fire) réaliste
- PERSISTANCE: Sauvegarde automatique toutes les 5 min

NOUVEAU v8.4.1:
- Compteur NEURONS_TOTAL persisté (croissance cumulative)
- Sauvegarde automatique dans /mnt/nvme/soullink_brain/
- Chargement au démarrage si fichier existe
"""

from flask import Flask, Response
import json
import time
import threading
import random
import math
import os
from pathlib import Path

# ═══════════════════════════════════════════════════════════════════════════
# PERSISTANCE
# ═══════════════════════════════════════════════════════════════════════════

PERSIST_PATH = Path("/mnt/nvme/soullink_brain")
PERSIST_PATH.mkdir(parents=True, exist_ok=True)
STATE_FILE = PERSIST_PATH / "brain_state.json"

def load_state():
    """Charge l'état sauvegardé"""
    if STATE_FILE.exists():
        try:
            with open(STATE_FILE, 'r') as f:
                data = json.load(f)
                print(f"📂 État chargé: {data.get('total_neurons', 0)} neurones totaux, {data.get('growth_events', 0)} croissances")
                return data
        except:
            pass
    return {"total_neurons": 0, "growth_events": 0, "total_spikes": 0, "last_save": 0}

def save_state(state):
    """Sauvegarde l'état"""
    try:
        with open(STATE_FILE, 'w') as f:
            json.dump(state, f, indent=2)
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
    'language': ['memory', 'output', 'audio', 'reasoning'],
    'vision': ['perception', 'attention'],
    'audio': ['perception', 'language', 'attention'],
    'motor': ['output'],
}

# ═══════════════════════════════════════════════════════════════════════════
# ÉTAT DU RÉSEAU
# ═══════════════════════════════════════════════════════════════════════════

class Brain:
    def __init__(self):
        self.modules = {}
        self.neurons = []
        self.synapses = []
        self.adj = {}
        self.pending = []
        self.signals = []
        self.sim_time = 0
        self.stats = {'N': 0, 'syn': 0, 'spk': 0, 'sig': 0, 'hz': 0, 'growth': 0}
        self.lock = threading.Lock()
        
        # PERSISTANCE: Charger l'état précédent
        self.persistent = load_state()
        self.total_neurons = self.persistent.get('total_neurons', 0)
        self.growth_events = self.persistent.get('growth_events', 0)
        self.total_spikes = self.persistent.get('total_spikes', 0)
        self.last_save_time = time.time()
        
        self._init_modules()
        self._init_neurons()
        self._init_synapses()
        
        # Restaurer le growth depuis la persistance
        self.stats['growth'] = self.growth_events
        # Le total_neurons vient UNIQUEMENT de la persistance (ou stats['N'] au premier démarrage)
        if self.total_neurons == 0:
            self.total_neurons = self.stats['N']
        print(f"🧠 État initial: {self.stats['N']} neurones actuels, {self.total_neurons} total historique (persisté: {self.growth_events} croissances)")

    def _init_modules(self):
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

    def _init_neurons(self):
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

    def _init_synapses(self):
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

    def step(self):
        """Simulation LIF - un step"""
        self.sim_time += DT
        spikes = 0
        new_signals = []

        # Process pending signals
        for i in range(len(self.pending) - 1, -1, -1):
            e = self.pending[i]
            if self.sim_time >= e['at']:
                e['t']['v'] += e['w'] * 6
                self.pending.pop(i)

        # Neuron updates
        for mod in self.modules.values():
            for n in mod['neurons']:
                n['sp'] = False
                if (self.sim_time - n['tL']) < TREF:
                    n['v'] = VZ
                    n['glow'] = max(0, n['glow'] * 0.82)
                    continue

                n['v'] += (-(n['v'] - VR) / TAU + n['drive']) * DT + random.gauss(0, 0.35)
                n['vn'] = max(0, min(1, (n['v'] - VR) / (VT - VR)))

                if n['v'] >= VT:
                    n['sp'] = True
                    n['tL'] = self.sim_time
                    n['v'] = VZ
                    n['glow'] = 1
                    n['fc'] += 1
                    spikes += 1

                    for conn in self.adj[n['id']]:
                        if len(self.pending) < 2000:
                            self.pending.append({
                                'at': self.sim_time + conn['d'],
                                't': conn['t'],
                                'w': conn['w']
                            })
                            # Créer un signal animé
                            if len(self.signals) < 500:
                                new_signals.append({
                                    'from_mod': n['mod'],
                                    'to_mod': conn['t']['mod'],
                                    'progress': 0,
                                    'color': self.modules[n['mod']]['color'],
                                    'weight': conn['w'],
                                })
                else:
                    n['glow'] = max(0, n['glow'] * 0.91)

        # Mettre à jour les signaux animés
        for sig in self.signals:
            sig['progress'] += 0.03
        self.signals = [s for s in self.signals if s['progress'] < 1]
        self.signals.extend(new_signals[:50])  # Max 50 nouveaux par frame

        self.stats['spk'] = spikes
        self.stats['sig'] = len(self.pending)
        self.stats['hz'] = round(spikes / (self.stats['N'] * DT / 1000), 1)

    def grow(self):
        """Add a new neuron"""
        mod = random.choice(list(self.modules.values()))
        i = len(mod['neurons'])
        is_inhib = random.random() < 0.2
        r = 52 + mod['n'] * 1.4

        neuron = {
            'id': f"{mod['name'][:3]}_{i:03d}",
            'mod': mod['name'],
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
        mod['neurons'].append(neuron)
        self.adj[neuron['id']] = []

        other_mod = random.choice(list(self.modules.values()))
        if other_mod['neurons']:
            other = random.choice(other_mod['neurons'])
            w = 0.15 + random.random() * 0.55
            if not neuron['exc']:
                w *= -0.7
            d = 1 + random.random() * 8
            self.synapses.append({'s': neuron, 't': other, 'w': w, 'd': d})
            self.adj[neuron['id']].append({'t': other, 'w': w, 'd': d})

            w2 = 0.15 + random.random() * 0.55
            d2 = 1 + random.random() * 8
            self.synapses.append({'s': other, 't': neuron, 'w': w2, 'd': d2})
            self.adj[other['id']].append({'t': neuron, 'w': w2, 'd': d2})

        self.stats['N'] = len(self.neurons)
        self.stats['syn'] = len(self.synapses)
        self.stats['growth'] += 1
        
        # PERSISTANCE: total_neurons est le MAXIMUM atteint (jamais redescend)
        self.total_neurons = max(self.total_neurons, self.stats['N'])
        self.growth_events = self.stats['growth']

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
            } for s in self.signals[:100]],  # Max 100 signaux
            'stats': {
                'N': self.stats['N'],
                'syn': self.stats['syn'],
                'spk': self.stats['spk'],
                'sig': len(self.signals),
                'hz': self.stats['hz'],
                'growth': self.stats['growth'],
                'total_neurons': self.total_neurons,  # PERSISTANCE: total historique
                'total_spikes': self.total_spikes,     # PERSISTANCE: spikes cumulés
            },
            'sim_time': round(self.sim_time, 1),
            'wires': WIRES,
        }


brain = Brain()

# ═══════════════════════════════════════════════════════════════════════════
# FLASK ROUTES
# ═══════════════════════════════════════════════════════════════════════════

HTML_TEMPLATE = '''<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>🧠 SoulLink Brain v8.4 — 3D Bubbles ULTIME</title>
<style>
*{margin:0;padding:0;box-sizing:border-box;}
html,body{height:100%;overflow:hidden;}
#w{position:fixed;top:0;left:0;width:100%;height:100vh;background:#06060b;overflow:hidden;}
#w.drg{cursor:grabbing;}
#c{display:block;width:100%;height:100%;cursor:grab;}
.pn{position:absolute;background:rgba(4,4,20,.92);border:1px solid rgba(61,255,192,.22);border-radius:10px;padding:11px 16px;font:11px/1.85 'JetBrains Mono',monospace;color:#3dffc0;pointer-events:none;}
.pv{color:#fff;font-weight:700;}
.pd{color:rgba(61,255,192,.45);}
#hud{top:12px;left:12px;}
#nfo{top:12px;right:12px;width:206px;display:none;}
#nfo-title{font-weight:700;font-size:13px;margin-bottom:7px;display:flex;align-items:center;gap:8px;}
#nfo-dot{width:10px;height:10px;border-radius:50%;flex-shrink:0;}
#legend{bottom:12px;left:12px;display:flex;flex-wrap:wrap;gap:4px;max-width:340px;padding:7px 9px;}
.lb{padding:2px 8px;border-radius:4px;font:8.5px/1.5 'JetBrains Mono',monospace;font-weight:700;cursor:pointer;pointer-events:all;}
#hint{position:absolute;bottom:14px;left:50%;transform:translateX(-50%);color:rgba(61,255,192,.22);font:8.5px 'JetBrains Mono',monospace;letter-spacing:.2em;text-transform:uppercase;pointer-events:none;white-space:nowrap;}
#loading{position:absolute;top:50%;left:50%;transform:translate(-50%,-50%);color:#3dffc0;font:14px 'JetBrains Mono',monospace;}
#activity{position:absolute;top:12px;left:50%;transform:translateX(-50%);color:#3dffc0;font:10px 'JetBrains Mono',monospace;opacity:0.7;}
</style>
</head>
<body>
<div id="w">
<canvas id="c"></canvas>
<div class="pn" id="hud">
  🧠 <span class="pv" id="hN">0</span>N <span class="pd">(Σ <span class="pv" id="hTotal">0</span>)</span> &nbsp;<span class="pd">|</span>&nbsp; ⚡ <span class="pv" id="hS">0</span>spk &nbsp;<span class="pd">|</span>&nbsp; 📡 <span class="pv" id="hG">0</span>sig<br>
  🔗 <span class="pv" id="hSyn">0</span>syn &nbsp;<span class="pd">|</span>&nbsp; 📈 <span class="pv" id="hHz">0</span>Hz &nbsp;<span class="pd">|</span>&nbsp; ⏱ <span class="pv" id="hT">0</span>s &nbsp;<span class="pd">|</span>&nbsp; 🌱 <span class="pv" id="hGr">0</span>
</div>
<div class="pn" id="nfo">
  <div id="nfo-title"><div id="nfo-dot"></div><span id="nfo-name">—</span></div>
  <div id="nfo-body"></div>
</div>
<div class="pn" id="legend"></div>
<div id="activity">⚡ Active: <span id="actCount">0</span> neurons</div>
<div id="hint">Drag · Scroll · Cliquer une bulle</div>
<div id="loading">⏳ Chargement...</div>
</div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>
<script>
console.log('🧠 Brain v8.4 - ULTIME - Démarrage');

let brainData=null,initialized=false;
const neuronMeshMap={},moduleGroups={},clickable=[],signalMeshes=[];

async function pollBrain(){
  try{
    const res=await fetch('/api/brain');
    brainData=await res.json();
    if(!initialized&&brainData.neurons.length>0){
      console.log('📊 Données reçues:',brainData.stats.N,'neurones');
      initScene();
      initSignalPool();
      document.getElementById('loading').style.display='none';
      initialized=true;
    }
    if(initialized)updateFromData();
  }catch(e){console.error('❌ Polling error:',e);}
}

setInterval(pollBrain,50);
pollBrain();

const wrap=document.getElementById('w'),canvas=document.getElementById('c'),CW=window.innerWidth,CH=window.innerHeight;
const renderer=new THREE.WebGLRenderer({canvas,antialias:true,alpha:false});
renderer.setSize(CW,CH);
renderer.setPixelRatio(Math.min(window.devicePixelRatio,2));

const scene=new THREE.Scene();
scene.background=new THREE.Color(0x06060b);
scene.fog=new THREE.FogExp2(0x06060b,.0008);

const camera=new THREE.PerspectiveCamera(52,CW/CH,1,5000);
let theta=.42,phi=1.08,radius=680,tTheta=.42,tPhi=1.08,tRadius=680;

function camUpdate(){
  theta+=(tTheta-theta)*.09;
  phi+=(tPhi-phi)*.09;
  radius+=(tRadius-radius)*.09;
  camera.position.set(radius*Math.sin(phi)*Math.sin(theta),radius*Math.cos(phi),radius*Math.sin(phi)*Math.cos(theta));
  camera.lookAt(0,0,0);
}

scene.add(new THREE.AmbientLight(0x1a2255,.8));
const sun=new THREE.PointLight(0x4488ff,.5,2000);
sun.position.set(0,350,250);
scene.add(sun);
const rim=new THREE.PointLight(0x3dffc0,.25,1800);
rim.position.set(-280,-80,-180);
scene.add(rim);

// Stars
(function(){
  const sg=new THREE.BufferGeometry(),sp=new Float32Array(3600);
  for(let i=0;i<1200;i++){sp[i*3]=(Math.random()-.5)*3200;sp[i*3+1]=(Math.random()-.5)*3200;sp[i*3+2]=(Math.random()-.5)*3200;}
  sg.setAttribute('position',new THREE.BufferAttribute(sp,3));
  scene.add(new THREE.Points(sg,new THREE.PointsMaterial({color:0xffffff,size:1.6,transparent:true,opacity:.55})));
})();

function h2n(h){return parseInt(h.replace('#',''),16);}

// Signal pool - particules pour les signaux inter-modules
function initSignalPool(){
  const sigGeo=new THREE.SphereGeometry(4,8,8);
  for(let i=0;i<300;i++){
    const mesh=new THREE.Mesh(sigGeo,new THREE.MeshPhongMaterial({
      color:0xffffff,
      emissive:0xaaffcc,
      emissiveIntensity:1,
      transparent:true,
      opacity:.9,
      depthWrite:false
    }));
    mesh.visible=false;
    scene.add(mesh);
    signalMeshes.push(mesh);
  }
}

function initScene(){
  console.log('🎬 initScene() - Création des modules et neurones');
  
  for(const[nm,conf] of Object.entries(brainData.modules)){
    const n=conf.n,col=conf.color,[px,py,pz]=conf.pos;
    const r=52+n*1.4,hexC=h2n(col);
    const grp=new THREE.Group();
    grp.position.set(px,py,pz);
    grp.userData={baseY:py,driftPhase:conf.driftPhase};
    
    const fill=new THREE.Mesh(new THREE.SphereGeometry(r,32,24),new THREE.MeshPhongMaterial({color:hexC,transparent:true,opacity:.06,depthWrite:false,side:THREE.DoubleSide,shininess:140}));
    fill.userData={type:'module',name:nm};
    grp.add(fill);
    clickable.push(fill);
    
    // Inner sphere
    grp.add(new THREE.Mesh(new THREE.SphereGeometry(r*.7,24,18),new THREE.MeshPhongMaterial({color:hexC,transparent:true,opacity:.04,depthWrite:false,side:THREE.BackSide})));
    
    // Wireframe
    grp.add(new THREE.Mesh(new THREE.SphereGeometry(r,12,10),new THREE.MeshPhongMaterial({color:hexC,transparent:true,opacity:.18,depthWrite:false,wireframe:true})));
    
    // Inner glow
    grp.add(new THREE.Mesh(new THREE.SphereGeometry(r*.4,10,10),new THREE.MeshPhongMaterial({color:hexC,transparent:true,opacity:.08,depthWrite:false})));
    
    const light=new THREE.PointLight(hexC,.25,r*4);
    grp.add(light);
    
    scene.add(grp);
    moduleGroups[nm]={grp,fill,baseY:py,r,light,neurons:[]};
  }
  
  // Connexions inter-modules (lignes)
  (function(){
    const pos=[],col_=[];
    for(const[sn,tgts] of Object.entries(brainData.wires)){
      const sp=brainData.modules[sn].pos,sc=new THREE.Color(h2n(brainData.modules[sn].color));
      for(const tn of tgts){
        const tp=brainData.modules[tn].pos;
        pos.push(sp[0],sp[1],sp[2],tp[0],tp[1],tp[2]);
        col_.push(sc.r,sc.g,sc.b,sc.r,sc.g,sc.b);
      }
    }
    const geo=new THREE.BufferGeometry();
    geo.setAttribute('position',new THREE.BufferAttribute(new Float32Array(pos),3));
    geo.setAttribute('color',new THREE.BufferAttribute(new Float32Array(col_),3));
    scene.add(new THREE.LineSegments(geo,new THREE.LineBasicMaterial({vertexColors:true,transparent:true,opacity:.22,depthWrite:false})));
  })();
  
  console.log('🔬 Création de',brainData.neurons.length,'neurones');
  for(const n of brainData.neurons){
    createNeuronMesh(n);
  }
  console.log('✅',Object.keys(neuronMeshMap).length,'neurones créés');
  
  const leg=document.getElementById('legend');
  leg.innerHTML=Object.entries(brainData.modules).map(([nm,conf])=>`<div class="lb" style="background:${conf.color}1a;border:1px solid ${conf.color}44;color:${conf.color}">${nm.slice(0,4).toUpperCase()}</div>`).join('');
}

function createNeuronMesh(n){
  const mod=brainData.modules[n.mod],mg=moduleGroups[n.mod];
  if(!mod||!mg)return null;
  
  const col=mod.color,hexC=h2n(col),baseSize=2+(n.imp||0.5)*2;
  const mesh=new THREE.Mesh(new THREE.SphereGeometry(baseSize,8,6),new THREE.MeshPhongMaterial({color:hexC,emissive:hexC,emissiveIntensity:.08,shininess:90,transparent:true,opacity:.65}));
  mesh.position.set(n.rx,n.ry,n.rz);
  mesh.userData={type:'neuron',nid:n.id,n:n};
  mg.grp.add(mesh);
  mg.neurons.push(mesh);
  neuronMeshMap[n.id]=mesh;
  clickable.push(mesh);
  return mesh;
}

function updateFromData(){
  if(!brainData||!initialized)return;
  
  document.getElementById('hN').textContent=brainData.stats.N;
  document.getElementById('hTotal').textContent=brainData.stats.total_neurons || brainData.stats.N;
  document.getElementById('hS').textContent=brainData.stats.spk;
  document.getElementById('hG').textContent=brainData.stats.sig;
  document.getElementById('hSyn').textContent=brainData.stats.syn;
  document.getElementById('hHz').textContent=brainData.stats.hz;
  document.getElementById('hT').textContent=(brainData.sim_time/1000).toFixed(1);
  document.getElementById('hGr').textContent=brainData.stats.growth;
  
  // Active neurons count
  const activeCount=brainData.neurons.filter(n=>n.glow>0.1||n.sp).length;
  document.getElementById('actCount').textContent=activeCount;
  
  // New neurons
  const currentCount=Object.keys(neuronMeshMap).length;
  if(brainData.neurons.length>currentCount){
    for(const n of brainData.neurons){
      if(!neuronMeshMap[n.id])createNeuronMesh(n);
    }
  }
  
  // Update neurons
  for(const n of brainData.neurons){
    const mesh=neuronMeshMap[n.id];
    if(!mesh)continue;
    const mod=brainData.modules[n.mod];
    if(!mod)continue;
    
    if(n.sp){
      mesh.material.color.set(0xffffff);
      mesh.material.emissive.set(0xffffff);
      mesh.material.emissiveIntensity=1.2;
      mesh.scale.setScalar(2.8);
    }else if(n.glow>0.05){
      mesh.material.color.setStyle(mod.color);
      mesh.material.emissive.setStyle(mod.color);
      mesh.material.emissiveIntensity=n.glow*.9;
      mesh.scale.setScalar(1+n.glow*.6);
    }else{
      mesh.material.color.setStyle(mod.color);
      mesh.material.emissive.setStyle(mod.color);
      mesh.material.emissiveIntensity=.05+n.vn*.25;
      mesh.scale.setScalar(1);
    }
    mesh.material.opacity=.5+n.vn*.5;
    mesh.userData.n=n;
  }
  
  // Update signals (particules)
  let sigIdx=0;
  for(const sig of(brainData.signals||[])){
    if(sigIdx>=signalMeshes.length)break;
    const fromMod=brainData.modules[sig.from_mod];
    const toMod=brainData.modules[sig.to_mod];
    if(!fromMod||!toMod)continue;
    
    const mesh=signalMeshes[sigIdx++];
    const fp=fromMod.pos,tp=toMod.pos;
    const prog=sig.progress;
    
    // Position interpolée
    mesh.position.set(
      fp[0]+(tp[0]-fp[0])*prog,
      fp[1]+(tp[1]-fp[1])*prog,
      fp[2]+(tp[2]-fp[2])*prog
    );
    mesh.material.color.setStyle(sig.color);
    mesh.material.emissive.setStyle(sig.color);
    mesh.material.opacity=.9*(1-prog);
    mesh.scale.setScalar(1.5+(1-prog)*.5);
    mesh.visible=true;
  }
  // Hide unused
  for(let i=sigIdx;i<signalMeshes.length;i++){
    signalMeshes[i].visible=false;
  }
  
  // Update lights
  for(const[nm,mg]of Object.entries(moduleGroups)){
    const mod=brainData.modules[nm];
    if(!mod)continue;
    const avgGlow=brainData.neurons.filter(n=>n.mod===nm).reduce((s,n)=>s+n.glow,0)/mod.n;
    mg.light.intensity=.15+avgGlow*3;
  }
}

let drag=false,downX=0,downY=0,prevX=0,prevY=0;
wrap.addEventListener('mousedown',e=>{drag=true;downX=prevX=e.clientX;downY=prevY=e.clientY;wrap.classList.add('drg');});
window.addEventListener('mouseup',()=>{drag=false;wrap.classList.remove('drg');});
window.addEventListener('mousemove',e=>{if(!drag)return;tTheta-=(e.clientX-prevX)*.009;tPhi=Math.max(.12,Math.min(Math.PI-.12,tPhi+(e.clientY-prevY)*.009));prevX=e.clientX;prevY=e.clientY;});
wrap.addEventListener('wheel',e=>{e.preventDefault();tRadius=Math.max(120,Math.min(1600,tRadius+e.deltaY*.65));},{passive:false});

let ptd=0;
wrap.addEventListener('touchstart',e=>{if(e.touches.length===1){drag=true;downX=prevX=e.touches[0].clientX;downY=prevY=e.touches[0].clientY;}else if(e.touches.length===2){const dx=e.touches[0].clientX-e.touches[1].clientX,dy=e.touches[0].clientY-e.touches[1].clientY;ptd=Math.sqrt(dx*dx+dy*dy);drag=false;}},{passive:true});
wrap.addEventListener('touchmove',e=>{if(e.touches.length===1&&drag){tTheta-=(e.touches[0].clientX-prevX)*.009;tPhi=Math.max(.12,Math.min(Math.PI-.12,tPhi+(e.touches[0].clientY-prevY)*.009));prevX=e.touches[0].clientX;prevY=e.touches[0].clientY;}else if(e.touches.length===2){const dx=e.touches[0].clientX-e.touches[1].clientX,dy=e.touches[0].clientY-e.touches[1].clientY,d=Math.sqrt(dx*dx+dy*dy);tRadius=Math.max(120,Math.min(1600,tRadius+(ptd-d)*1.4));ptd=d;}},{passive:true});
wrap.addEventListener('touchend',()=>{drag=false;});

const raycaster=new THREE.Raycaster(),mVec=new THREE.Vector2();
wrap.addEventListener('click',e=>{
  if(Math.abs(e.clientX-downX)>8||Math.abs(e.clientY-downY)>8)return;
  const rect=wrap.getBoundingClientRect();
  mVec.x=((e.clientX-rect.left)/rect.width)*2-1;
  mVec.y=-((e.clientY-rect.top)/rect.height)*2+1;
  raycaster.setFromCamera(mVec,camera);
  const hits=raycaster.intersectObjects(clickable,false);
  if(hits.length){
    const ud=hits[0].object.userData;
    if(ud.type==='module')showMod(ud.name);
    else if(ud.type==='neuron')showNeu(ud.n);
    document.getElementById('hint').style.display='none';
  }else document.getElementById('nfo').style.display='none';
});

function showMod(nm){
  if(!brainData)return;
  const m=brainData.modules[nm];
  const exc=brainData.neurons.filter(n=>n.mod===nm&&n.exc).length;
  const avgHz=(brainData.neurons.filter(n=>n.mod===nm).reduce((s,n)=>s+n.fc,0)/(m.n*Math.max(brainData.sim_time/1000,.01))).toFixed(1);
  const avgGlow=(brainData.neurons.filter(n=>n.mod===nm).reduce((s,n)=>s+n.glow,0)/m.n).toFixed(2);
  document.getElementById('nfo-dot').style.background=m.color;
  document.getElementById('nfo-name').textContent=nm.toUpperCase();
  document.getElementById('nfo-body').innerHTML=`<span class="pd">Neurones</span> <span class="pv">${m.n}</span><br><span class="pd">Excitateurs</span> <span class="pv">${exc}</span> <span class="pd">/ Inhib.</span> <span class="pv">${m.n-exc}</span><br><span class="pd">Hz moyen</span> <span class="pv">${avgHz}</span><br><span class="pd">Glow actuel</span> <span class="pv">${avgGlow}</span><br><span class="pd">→</span> <span class="pv" style="font-size:9.5px">${(brainData.wires[nm]||[]).join(', ')}</span>`;
  document.getElementById('nfo').style.display='block';
}

function showNeu(n){
  if(!brainData)return;
  const m=brainData.modules[n.mod];
  document.getElementById('nfo-dot').style.background=m.color;
  document.getElementById('nfo-name').textContent=n.id;
  document.getElementById('nfo-body').innerHTML=`<span class="pd">Module</span> <span class="pv">${n.mod}</span><br><span class="pd">Type</span> <span class="pv">${n.exc?'Excitateur':'Inhibiteur'}</span><br><span class="pd">Potentiel V</span> <span class="pv">${(-70+n.vn*55).toFixed(1)} mV</span><br><span class="pd">Drive I</span> <span class="pv">${(n.drive||0).toFixed(2)}</span><br><span class="pd">Importance</span> <span class="pv">${(n.imp||0).toFixed(2)}</span><br><span class="pd">Spikes total</span> <span class="pv">${n.fc}</span>`;
  document.getElementById('nfo').style.display='block';
}

function animate(ts){
  requestAnimationFrame(animate);
  const t=ts*.001;
  if(brainData){
    for(const[nm,mg]of Object.entries(moduleGroups)){
      const conf=brainData.modules[nm],phase=mg.grp.userData.driftPhase;
      mg.grp.position.y=mg.grp.userData.baseY+Math.sin(t*.42+phase)*8;
      mg.grp.rotation.y=t*.038+phase;
      mg.grp.rotation.x=Math.sin(t*.028+phase)*.05;
    }
  }
  camUpdate();
  renderer.render(scene,camera);
}

requestAnimationFrame(animate);
console.log('🎬 Animation loop démarré');
</script>
</body>
</html>'''


@app.route('/')
def index():
    """Page principale avec visualisation 3D"""
    return Response(HTML_TEMPLATE, mimetype='text/html')


@app.route('/api/brain')
def api_brain():
    """API JSON pour l'état du brain"""
    with brain.lock:
        return Response(json.dumps(brain.to_json()), mimetype='application/json')


@app.route('/api/stats')
def api_stats():
    """API JSON pour les stats uniquement"""
    with brain.lock:
        return {
            'neurons': brain.stats['N'],
            'synapses': brain.stats['syn'],
            'spikes': brain.stats['spk'],
            'signals': brain.stats['sig'],
            'hz': brain.stats['hz'],
            'growth': brain.stats['growth'],
            'sim_time': brain.sim_time,
        }


def simulation_loop():
    """Boucle de simulation en arrière-plan"""
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
            
            # Sauvegarde toutes les 30s
            if time.time() - last_save > 30:
                brain.persistent['total_neurons'] = brain.total_neurons
                brain.persistent['growth_events'] = brain.growth_events
                brain.persistent['total_spikes'] = brain.total_spikes
                brain.persistent['last_save'] = time.time()
                save_state(brain.persistent)
                last_save = time.time()
        time.sleep(0.002)


if __name__ == '__main__':
    print("🧠 SoulLink Brain v8.4.1 — 3D Bubbles ULTIME + PERSISTANCE")
    print(f"   Modules: {len(brain.modules)}")
    print(f"   Neurones actuels: {brain.stats['N']}")
    print(f"   Neurones totaux: {brain.total_neurons} (historique)")
    print(f"   Synapses: {brain.stats['syn']}")
    print(f"   Port: 8084")
    print("")
    print("   🌐 Interface: http://localhost:8084")
    print("   📊 API: http://localhost:8084/api/brain")
    print("   💾 Persistance: /mnt/nvme/soullink_brain/brain_state.json")
    print("   ✨ Features: Neurones animés + Signaux inter-modules + Persistance")

    sim_thread = threading.Thread(target=simulation_loop, daemon=True)
    sim_thread.start()

    app.run(host='0.0.0.0', port=8084, threaded=True)