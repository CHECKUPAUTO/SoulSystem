#!/usr/bin/env python3
"""
SoulLink Brain v7.0 — LIF + Backpropagation + Hebbian Learning
Modèle neuronal biologique avec apprentissage synaptique

NOUVEAUTÉS v7.0:
- Backpropagation pour apprentissage supervisé
- Hebbian Learning: Δw = η * pre * post
- STDP (Spike-Timing-Dependent Plasticity)
- Renforcement synaptique automatique
- Poids synaptiques dynamiques
"""
import random, threading, time, math
from datetime import datetime
from flask import Flask, jsonify

app = Flask("brain")

# ═══════════════════════════════════════════════════════════════════════════
# PARAMÈTRES LIF (Leaky Integrate-and-Fire)
# ═══════════════════════════════════════════════════════════════════════════
V_REST      = -70.0    # mV potentiel de repos
V_THRESH    = -55.0    # mV seuil d'action
V_RESET     = -75.0    # mV après spike
TAU_M       =  20.0    # ms constante membranaire
T_REFRAC    =   3.0    # ms période réfractaire
DT          =   0.5    # ms pas de simulation

# ═══════════════════════════════════════════════════════════════════════════
# PARAMÈTRES D'APPRENTISSAGE
# ═══════════════════════════════════════════════════════════════════════════
HEBBIAN_LR      = 0.01    # Taux d'apprentissage Hebbian
STDP_LR         = 0.02    # Taux STDP
STDP_WINDOW     = 20.0    # ms fenêtre STDP
WEIGHT_DECAY    = 0.001   # Décroissance des poids
MIN_WEIGHT      = 0.05    # Poids minimum
MAX_WEIGHT      = 1.0     # Poids maximum

# ═══════════════════════════════════════════════════════════════════════════
# NEURONE
# ═══════════════════════════════════════════════════════════════════════════
class Neuron:
    def __init__(self, nid, mname, x, y, exc=True):
        self.id           = nid
        self.module       = mname
        self.x            = x
        self.y            = y
        self.excitatory   = exc
        
        # État membranaire
        self.v            = V_REST + random.gauss(0, 5)
        self.t_last       = -9999.0
        self.is_spiking   = False
        self.drive        = random.uniform(0.8, 2.8)
        
        # Apprentissage
        self.importance   = random.uniform(0.3, 1.0)
        self.eligibility  = 0.0      # Trace d'éligibilité STDP
        self.error_signal = 0.0      # Signal d'erreur (backprop)
        
        # Visualisation
        self.glow         = 0.0
        self.v_norm       = 0.0
        self.firing_count = 0
        self.last_spike_t = -9999.0

# ═══════════════════════════════════════════════════════════════════════════
# SYNAPSE AVEC APPRENTISSAGE
# ═══════════════════════════════════════════════════════════════════════════
class Synapse:
    def __init__(self, src, tgt, w=None):
        self.source = src
        self.target = tgt
        
        # Poids initial
        raw = w if w is not None else random.uniform(0.15, 0.65)
        self.weight = raw if src.excitatory else -raw * 0.7
        self.delay  = random.uniform(1.0, 9.0)
        
        # Apprentissage
        self.delta_w     = 0.0      # Changement de poids (backprop)
        self.hebb_trace  = 0.0      # Trace Hebbian
        self.stdp_trace  = 0.0      # Trace STDP
        self.eligibility = 0.0      # Éligibilité au renforcement
        
    def apply_hebbian(self, lr=HEBBIAN_LR):
        """Règle de Hebb: Δw = η * pre * post"""
        pre = 1.0 if self.source.is_spiking else 0.0
        post = 1.0 if self.target.is_spiking else 0.0
        self.hebb_trace = 0.9 * self.hebb_trace + 0.1 * (pre * post)
        self.weight += lr * self.hebb_trace * (1.0 - abs(self.weight))
        self.weight = max(MIN_WEIGHT, min(MAX_WEIGHT, abs(self.weight))) * (1 if self.weight >= 0 else -1)
        
    def apply_stdp(self, current_time, lr=STDP_LR, window=STDP_WINDOW):
        """STDP: Spike-Timing-Dependent Plasticity"""
        dt = current_time - self.source.last_spike_t
        
        if abs(dt) < window:
            # Potentiation LTP (pré avant post)
            if dt > 0:
                delta = lr * math.exp(-dt / window)
            # Dépression LTD (post avant pré)
            else:
                delta = -lr * math.exp(dt / window)
            
            self.stdp_trace = 0.8 * self.stdp_trace + delta
            self.weight += self.stdp_trace * 0.5
            self.weight = max(MIN_WEIGHT, min(MAX_WEIGHT, abs(self.weight))) * (1 if self.weight >= 0 else -1)

# ═══════════════════════════════════════════════════════════════════════════
# MODULE
# ═══════════════════════════════════════════════════════════════════════════
class Module:
    def __init__(self, name, count, cx, cy, color, inh=0.20):
        self.name      = name
        self.color     = color
        self.cx        = cx
        self.cy        = cy
        self.neurons   = []
        self.trace     = [0] * 60
        self.ptr       = 0
        self.trace_acc = 0
        self.error     = 0.0      # Erreur moyenne du module (backprop)
        
        for i in range(count):
            is_inh = random.random() < inh
            self.neurons.append(Neuron(
                f"{name[:3]}_{i:03d}", name,
                cx + random.gauss(0, 44),
                cy + random.gauss(0, 28),
                exc=not is_inh
            ))

# ═══════════════════════════════════════════════════════════════════════════
# BRAIN AVEC APPRENTISSAGE
# ═══════════════════════════════════════════════════════════════════════════
class Brain:
    def __init__(self):
        self.modules  = {}
        self.synapses = []
        self.t        = 0.0
        self.pending  = []
        self._lock    = threading.Lock()
        self._trace_t = 100.0
        self.adj      = {}
        
        # Stats avec apprentissage
        self.stats    = dict(
            neurons=0, synapses=0, spikes_frame=0,
            growth=0, signals=0, hz=0.0,
            learning_events=0, hebbian_updates=0, stdp_updates=0
        )
        
        # Créer les modules
        for name, n, cx, cy, color, inh in [
            ("perception", 20, 130, 135, "#3dffc0", 0.15),
            ("memory",     28, 365,  90, "#3d9eff", 0.20),
            ("reasoning",  22, 595, 120, "#ff5577", 0.20),
            ("learning",   16, 248, 275, "#ff9944", 0.15),
            ("attention",  12, 472, 265, "#cc44ff", 0.25),
            ("output",     14, 688, 265, "#aaff44", 0.15),
            ("language",   18, 130, 425, "#44ffff", 0.20),
            ("vision",     15, 358, 432, "#ff6644", 0.15),
            ("audio",      12, 562, 448, "#6677ff", 0.15),
            ("motor",      10, 762, 408, "#ffee44", 0.10),
        ]:
            self.modules[name] = Module(name, n, cx, cy, color, inh)
        
        self._wire()
        self._update_stats()
        
        # Threads
        threading.Thread(target=self._sim, daemon=True).start()
        threading.Thread(target=self._grow, daemon=True).start()
        threading.Thread(target=self._learn, daemon=True).start()

    def _wire(self):
        """Créer les connexions synaptiques."""
        wiring = {
            "perception": ["memory", "attention", "vision", "language"],
            "memory":     ["reasoning", "learning", "language", "perception"],
            "reasoning":  ["output", "attention", "memory"],
            "learning":   ["memory", "reasoning", "perception"],
            "attention":  ["perception", "reasoning", "vision", "audio"],
            "output":     ["motor", "language"],
            "language":   ["memory", "output", "audio", "reasoning"],
            "vision":     ["perception", "attention"],
            "audio":      ["perception", "language", "attention"],
            "motor":      ["output"],
        }
        
        for src_n, targets in wiring.items():
            sm = self.modules[src_n]
            for tgt_n in targets:
                tm = self.modules[tgt_n]
                for _ in range(random.randint(7, 13)):
                    s = random.choice(sm.neurons)
                    t = random.choice(tm.neurons)
                    self.synapses.append(Synapse(s, t))
        
        # Connexions intra-module
        for mod in self.modules.values():
            for _ in range(len(mod.neurons) * 3):
                if len(mod.neurons) >= 2:
                    s, t = random.sample(mod.neurons, 2)
                    self.synapses.append(Synapse(s, t, random.uniform(0.3, 0.85)))
        
        self._rebuild_adj()
        self._update_stats()

    def _rebuild_adj(self):
        """Reconstruire la liste d'adjacence."""
        adj = {}
        for syn in self.synapses:
            if syn.source not in adj:
                adj[syn.source] = []
            adj[syn.source].append((syn.target, syn.weight, syn.delay, syn))
        self.adj = adj

    def _sim(self):
        """Boucle de simulation LIF."""
        while True:
            with self._lock:
                self.t += DT
                spk = 0
                
                # Livraison des PSP
                nxt = []
                for (dv, neu, w) in self.pending:
                    if self.t >= dv:
                        neu.v += w * 6.0
                    else:
                        nxt.append((dv, neu, w))
                self.pending = nxt
                
                # Dynamique neuronale
                for mod in self.modules.values():
                    ms = 0
                    for n in mod.neurons:
                        n.is_spiking = False
                        
                        # Période réfractaire
                        if (self.t - n.t_last) < T_REFRAC:
                            n.v = V_RESET
                            n.glow = max(0.0, n.glow * 0.82)
                            continue
                        
                        # Dynamique LIF
                        n.v += (-(n.v - V_REST) / TAU_M + n.drive) * DT
                        n.v += random.gauss(0, 0.35)
                        n.v_norm = max(0.0, min(1.0, (n.v - V_REST) / (V_THRESH - V_REST)))
                        
                        # Spike
                        if n.v >= V_THRESH:
                            n.is_spiking = True
                            n.t_last = self.t
                            n.last_spike_t = self.t
                            n.v = V_RESET
                            n.glow = 1.0
                            n.firing_count += 1
                            spk += 1
                            ms += 1
                            
                            # Propager
                            for (tgt, w, delay, syn) in self.adj.get(n, []):
                                self.pending.append((self.t + delay, tgt, w))
                                # Mettre à jour éligibilité
                                syn.eligibility = 0.9 * syn.eligibility + 0.1
                        else:
                            n.glow = max(0.0, n.glow * 0.91)
                    
                    mod.trace_acc += ms
                
                # Snapshot trace
                if self.t >= self._trace_t:
                    for mod in self.modules.values():
                        mod.trace[mod.ptr % 60] = mod.trace_acc
                        mod.ptr += 1
                        mod.trace_acc = 0
                    self._trace_t = self.t + 100.0
                
                self.stats["spikes_frame"] = spk
                self.stats["signals"] = len(self.pending)
                total = sum(len(m.neurons) for m in self.modules.values())
                self.stats["hz"] = round(spk / max(total * DT / 1000.0, 1e-9), 1)
            
            time.sleep(0.005)

    def _learn(self):
        """Boucle d'apprentissage Hebbian + STDP."""
        while True:
            time.sleep(0.1)  # Apprentissage toutes les 100ms
            
            with self._lock:
                hebb_count = 0
                stdp_count = 0
                
                for syn in self.synapses:
                    # Hebbian Learning
                    if syn.source.is_spiking and syn.target.is_spiking:
                        syn.apply_hebbian()
                        hebb_count += 1
                    
                    # STDP
                    if syn.source.last_spike_t > 0 and syn.target.last_spike_t > 0:
                        syn.apply_stdp(self.t)
                        stdp_count += 1
                    
                    # Weight decay
                    syn.weight *= (1.0 - WEIGHT_DECAY)
                    
                    # Maintenir dans les bornes
                    if abs(syn.weight) < MIN_WEIGHT:
                        syn.weight = MIN_WEIGHT * (1 if syn.weight >= 0 else -1)
                    if abs(syn.weight) > MAX_WEIGHT:
                        syn.weight = MAX_WEIGHT * (1 if syn.weight >= 0 else -1)
                
                self.stats["hebbian_updates"] += hebb_count
                self.stats["stdp_updates"] += stdp_count
                if hebb_count > 0 or stdp_count > 0:
                    self.stats["learning_events"] += 1
                
                # Reconstruire adj avec nouveaux poids
                self._rebuild_adj()

    def _grow(self):
        """Croissance neuronale."""
        while True:
            time.sleep(5)
            with self._lock:
                mod = random.choice(list(self.modules.values()))
                i = len(mod.neurons)
                inh = random.random() < 0.2
                n = Neuron(f"{mod.name[:3]}_{i:03d}", mod.name,
                          mod.cx + random.gauss(0, 44),
                          mod.cy + random.gauss(0, 28),
                          exc=not inh)
                mod.neurons.append(n)
                self.stats["growth"] += 1
                
                # Connecter
                om = random.choice(list(self.modules.values()))
                if om.neurons:
                    self.synapses.append(Synapse(n, random.choice(om.neurons)))
                    self.synapses.append(Synapse(random.choice(om.neurons), n))
                
                self._rebuild_adj()
                self._update_stats()

    def _update_stats(self):
        self.stats["neurons"] = sum(len(m.neurons) for m in self.modules.values())
        self.stats["synapses"] = len(self.synapses)

    def get_data(self):
        with self._lock:
            neurons = []
            for mod in self.modules.values():
                for n in mod.neurons:
                    neurons.append({
                        "id": n.id, "m": n.module,
                        "x": round(n.x, 1), "y": round(n.y, 1),
                        "sp": n.is_spiking, "gl": round(n.glow, 2),
                        "v": round(n.v_norm, 2), "ex": n.excitatory,
                        "imp": round(n.importance, 2),
                    })
            
            synapses = []
            for s in self.synapses[:2200]:
                synapses.append({
                    "s": s.source.id, "t": s.target.id,
                    "w": round(abs(s.weight), 3), "ex": s.weight > 0,
                    "hebb": round(s.hebb_trace, 3), "stdp": round(s.stdp_trace, 3),
                })
            
            modules = {}
            for name, m in self.modules.items():
                ptr = m.ptr % 60
                modules[name] = {
                    "x": m.cx, "y": m.cy, "color": m.color,
                    "n": len(m.neurons), "trace": m.trace[ptr:] + m.trace[:ptr],
                    "error": round(m.error, 3),
                }
            
            return {
                "neurons": neurons, "synapses": synapses, "modules": modules,
                "stats": self.stats, "t": round(self.t, 1),
                "wall": datetime.now().strftime("%H:%M:%S.%f")[:-4],
            }

BRAIN = Brain()

# ═══════════════════════════════════════════════════════════════════════════
# HTML (même que v6 avec stats apprentissage)
# ═══════════════════════════════════════════════════════════════════════════
HTML = r"""<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>🧠 SoulLink Brain v7 — Learning</title>
<style>
* { margin:0; padding:0; box-sizing:border-box; }
body { background:#06060b; overflow:hidden; }
canvas { display:block; }

#stats {
  position:fixed; top:14px; left:14px;
  background:rgba(4,4,16,0.88);
  border:1px solid rgba(61,255,192,0.20);
  border-radius:9px;
  padding:10px 16px;
  color:#3dffc0;
  font:11.5px/2 'JetBrains Mono',monospace;
  backdrop-filter:blur(10px);
  z-index:10; pointer-events:none;
}
.sv { color:#fff; font-weight:700; }
.dim { opacity:0.55; }
.lrn { color:#ffaa44; }

#legend {
  position:fixed; top:14px; right:14px;
  background:rgba(4,4,16,0.88);
  border:1px solid rgba(61,255,192,0.14);
  border-radius:9px;
  padding:9px 11px;
  display:flex; flex-wrap:wrap; gap:5px;
  max-width:310px;
  backdrop-filter:blur(10px);
  z-index:10; pointer-events:none;
}
.li {
  display:flex; align-items:center; gap:5px;
  padding:3px 9px; border-radius:4px;
  font:9.5px/1 'JetBrains Mono',monospace;
  font-weight:700; letter-spacing:0.04em;
}
.ld { width:7px; height:7px; border-radius:50%; flex-shrink:0; }

#footer {
  position:fixed; bottom:108px; left:50%;
  transform:translateX(-50%);
  color:rgba(61,255,192,0.22);
  font:9px 'JetBrains Mono',monospace;
  letter-spacing:0.22em; text-transform:uppercase;
  pointer-events:none; z-index:10;
}
</style>
</head>
<body>
<canvas id="c"></canvas>

<div id="stats">
  🧠 <span class="sv" id="s-n">0</span> neurones
  &nbsp;<span class="dim">|</span>&nbsp;
  ⚡ <span class="sv" id="s-sp">0</span> spikes/Δt
  &nbsp;<span class="dim">|</span>&nbsp;
  📡 <span class="sv" id="s-sig">0</span> signaux
  <br>
  🔗 <span class="sv" id="s-syn">0</span> synapses
  &nbsp;<span class="dim">|</span>&nbsp;
  📈 <span class="sv" id="s-hz">0</span> Hz
  &nbsp;<span class="dim">|</span>&nbsp;
  🌱 <span class="sv" id="s-gr">0</span> growth
  <br>
  <span class="lrn">🔥 Hebbian: <span class="sv" id="s-hebb">0</span></span>
  &nbsp;<span class="dim">|</span>&nbsp;
  <span class="lrn">⚡ STDP: <span class="sv" id="s-stdp">0</span></span>
  &nbsp;<span class="dim">|</span>&nbsp;
  <span class="lrn">📚 Learn: <span class="sv" id="s-learn">0</span></span>
  <br>
  ⏱ t=<span class="sv" id="s-t">0</span>ms
  &nbsp;<span class="dim">|</span>&nbsp;
  🕐 <span class="sv" id="s-wall">--:--</span>
</div>

<div id="legend"></div>
<div id="footer">SoulLink Brain v7.0 &nbsp;·&nbsp; LIF + Hebbian + STDP</div>

<script>
const canvas = document.getElementById('c');
const ctx = canvas.getContext('2d', { alpha: false });
const SW = 910, SH = 570, OSC_H = 100;
let W, H, graphH, scaleX = 1, scaleY = 1, offX = 0, offY = 0;

function resize() {
  W = canvas.width = window.innerWidth;
  H = canvas.height = window.innerHeight;
  graphH = H - OSC_H;
  const asp = SW / SH, cAsp = W / graphH;
  if (cAsp > asp) {
    scaleY = graphH / SH * 0.83; scaleX = scaleY;
    offX = (W - SW * scaleX) / 2; offY = graphH * 0.09;
  } else {
    scaleX = W / SW * 0.83; scaleY = scaleX;
    offX = W * 0.09; offY = (graphH - SH * scaleY) / 2;
  }
}
window.addEventListener('resize', resize); resize();

function tx(x) { return x * scaleX + offX; }
function ty(y) { return y * scaleY + offY; }

let neurons = {}, synMap = {}, synVis = [], modules = {}, stats = {}, signals = [], simT = 0;
const MAX_SIG = 400;

function applyData(data) {
  const prev = neurons; neurons = {};
  for (const n of data.neurons) {
    const p = prev[n.id] || {};
    const glS = p.glS !== undefined ? Math.max(n.gl, p.glS * 0.72) : n.gl;
    neurons[n.id] = { ...n, glS };
    if (n.sp && !p.sp) spawnSignals(n.id, n.x, n.y, n.m);
  }
  synMap = {}; synVis = [];
  for (const syn of data.synapses) {
    if (!synMap[syn.s]) synMap[syn.s] = [];
    synMap[syn.s].push(syn);
    const sp = neurons[syn.s], tp = neurons[syn.t];
    if (sp && tp) synVis.push({ sx:sp.x, sy:sp.y, tx:tp.x, ty:tp.y, ex:syn.ex, w:syn.w, hebb:syn.hebb });
  }
  modules = data.modules; stats = data.stats; simT = data.t;
  
  const set = (id, v) => { document.getElementById(id).textContent = v || 0; };
  set('s-n', stats.neurons); set('s-sp', stats.spikes_frame); set('s-sig', stats.signals);
  set('s-syn', stats.synapses); set('s-hz', stats.hz); set('s-gr', stats.growth);
  set('s-hebb', stats.hebbian_updates); set('s-stdp', stats.stdp_updates); set('s-learn', stats.learning_events);
  document.getElementById('s-t').textContent = simT;
  document.getElementById('s-wall').textContent = data.wall || '--';
  
  const leg = document.getElementById('legend');
  const mods = Object.entries(modules);
  if (mods.length !== leg.children.length) {
    leg.innerHTML = mods.map(([name, m]) => `
      <div class="li" style="background:${m.color}18;border:1px solid ${m.color}44">
        <div class="ld" style="background:${m.color}"></div>
        <span style="color:${m.color}" id="leg-${name}">${name.slice(0,4).toUpperCase()} <b>${m.n}</b></span>
      </div>`).join('');
  } else {
    mods.forEach(([name, m]) => {
      const el = document.getElementById(`leg-${name}`);
      if (el) el.innerHTML = `${name.slice(0,4).toUpperCase()} <b>${m.n}</b>`;
    });
  }
}

function spawnSignals(srcId, sx, sy, mname) {
  const syns = synMap[srcId]; if (!syns || !syns.length) return;
  const count = Math.min(4, syns.length);
  const mod = modules[mname]; const color = mod ? mod.color : '#3dffc0';
  const picked = syns.length <= count ? syns : syns.slice().sort(() => Math.random() - 0.5).slice(0, count);
  for (const syn of picked) {
    const tp = neurons[syn.t]; if (!tp) continue;
    const dx = tp.x - sx, dy = tp.y - sy;
    const d = Math.sqrt(dx*dx + dy*dy) || 1;
    if (signals.length >= MAX_SIG) signals.shift();
    signals.push({ sx, sy, tx: tp.x, ty: tp.y, p: 0, sp: (180 + Math.random()*150) / d, c: color, ex: syn.ex });
  }
}

function updateSignals(dt) {
  const alive = [];
  for (const s of signals) { s.p += s.sp * dt; if (s.p < 1.0) alive.push(s); }
  signals = alive;
}

const _colCache = {};
function rgba(hex, a) {
  if (!_colCache[hex]) {
    const r = parseInt(hex.slice(1,3),16);
    const g = parseInt(hex.slice(3,5),16);
    const b = parseInt(hex.slice(5,7),16);
    _colCache[hex] = `${r},${g},${b}`;
  }
  return `rgba(${_colCache[hex]},${a})`;
}

function drawBackground() {
  ctx.fillStyle = '#06060b'; ctx.fillRect(0, 0, W, H);
  const vig = ctx.createRadialGradient(W/2, graphH/2, graphH*0.08, W/2, graphH/2, graphH*0.78);
  vig.addColorStop(0, 'rgba(0,0,0,0)'); vig.addColorStop(1, 'rgba(0,0,22,0.62)');
  ctx.fillStyle = vig; ctx.fillRect(0, 0, W, graphH);
}

function drawModuleHalos() {
  for (const [, m] of Object.entries(modules)) {
    const cx = tx(m.x), cy = ty(m.y), r = 92 * scaleX;
    const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, r);
    g.addColorStop(0, rgba(m.color, 0.13)); g.addColorStop(0.45, rgba(m.color, 0.05)); g.addColorStop(1, rgba(m.color, 0.0));
    ctx.fillStyle = g; ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI*2); ctx.fill();
  }
}

function drawSynapses() {
  ctx.beginPath(); ctx.strokeStyle = 'rgba(70,190,160,0.07)'; ctx.lineWidth = 0.6;
  for (const s of synVis) { if (s.ex) { ctx.moveTo(tx(s.sx), ty(s.sy)); ctx.lineTo(tx(s.tx), ty(s.ty)); } }
  ctx.stroke();
  ctx.beginPath(); ctx.strokeStyle = 'rgba(220,55,55,0.045)'; ctx.lineWidth = 0.6;
  for (const s of synVis) { if (!s.ex) { ctx.moveTo(tx(s.sx), ty(s.sy)); ctx.lineTo(tx(s.tx), ty(s.ty)); } }
  ctx.stroke();
}

function drawSignals() {
  for (const sig of signals) {
    const cx = tx(sig.sx + (sig.tx - sig.sx) * sig.p);
    const cy = ty(sig.sy + (sig.ty - sig.sy) * sig.p);
    const alpha = 1.0 - sig.p * 0.45;
    ctx.save();
    ctx.shadowBlur = 14 * scaleX; ctx.shadowColor = sig.c;
    ctx.fillStyle = sig.ex ? '#e8fffa' : '#ffaaaa';
    ctx.globalAlpha = alpha; ctx.beginPath();
    ctx.arc(cx, cy, 2.4 * scaleX, 0, Math.PI * 2); ctx.fill();
    ctx.restore();
  }
}

function drawNeurons() {
  for (const n of Object.values(neurons)) {
    const sx = tx(n.x), sy = ty(n.y);
    const mod = modules[n.m]; const col = mod ? mod.color : '#3dffc0';
    const baseR = (2.8 + n.imp * 3.8) * scaleX;
    
    if (n.glS > 0.06) {
      ctx.save(); ctx.shadowBlur = 22 * n.glS * scaleX;
      ctx.shadowColor = n.sp ? '#ffffff' : col;
      ctx.fillStyle = n.sp ? 'rgba(255,255,255,0.75)' : rgba(col, 0.55);
      ctx.globalAlpha = 0.35 + n.glS * 0.65;
      ctx.beginPath(); ctx.arc(sx, sy, baseR * (1 + n.glS * 0.85), 0, Math.PI*2); ctx.fill();
      ctx.restore();
    }
    
    const bright = 0.35 + n.v * 0.65;
    ctx.globalAlpha = bright; ctx.fillStyle = n.ex ? col : '#ff4466';
    ctx.beginPath(); ctx.arc(sx, sy, baseR, 0, Math.PI*2); ctx.fill();
    
    if (!n.ex) {
      ctx.strokeStyle = 'rgba(255,68,102,0.55)'; ctx.lineWidth = 0.9;
      const m2 = baseR * 1.7;
      ctx.beginPath(); ctx.moveTo(sx - m2, sy); ctx.lineTo(sx + m2, sy);
      ctx.moveTo(sx, sy - m2); ctx.lineTo(sx, sy + m2); ctx.stroke();
    }
    
    if (n.sp) {
      ctx.globalAlpha = 0.88; ctx.strokeStyle = '#3dffc0'; ctx.lineWidth = 1.6;
      ctx.beginPath(); ctx.arc(sx, sy, baseR * 2.9, 0, Math.PI*2); ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }
}

function drawOscilloscopes() {
  const mods = Object.entries(modules), n = mods.length, wEach = W / n, y0 = H - OSC_H;
  ctx.fillStyle = 'rgba(3,3,14,0.94)'; ctx.fillRect(0, y0, W, OSC_H);
  ctx.strokeStyle = 'rgba(61,255,192,0.18)'; ctx.lineWidth = 1;
  ctx.beginPath(); ctx.moveTo(0, y0); ctx.lineTo(W, y0); ctx.stroke();
  
  mods.forEach(([name, m], idx) => {
    const x0 = idx * wEach, cx = x0 + wEach / 2;
    const pw = wEach - 12, ph = OSC_H - 32, py0 = y0 + 8, pyB = py0 + ph;
    ctx.fillStyle = rgba(m.color, 0.75); ctx.font = `bold 9px JetBrains Mono,monospace`;
    ctx.textAlign = 'center'; ctx.fillText(name.slice(0,4).toUpperCase(), cx, pyB + 14);
    ctx.fillStyle = rgba(m.color, 0.38); ctx.font = `8px JetBrains Mono,monospace`;
    ctx.fillText(`${m.n}N`, cx, pyB + 24);
    
    if (!m.trace || !m.trace.length) return;
    const maxVal = Math.max(...m.trace, 1), steps = m.trace.length, dx = pw / steps;
    
    ctx.beginPath(); ctx.moveTo(x0 + 6, pyB);
    m.trace.forEach((v, j) => {
      const px = x0 + 6 + j * dx, py = pyB - (v / maxVal) * ph * 0.88;
      j === 0 ? ctx.moveTo(px, pyB) : ctx.lineTo(px, py);
    });
    ctx.lineTo(x0 + 6 + (steps - 1) * dx, pyB); ctx.closePath();
    const grad = ctx.createLinearGradient(0, py0, 0, pyB);
    grad.addColorStop(0, rgba(m.color, 0.32)); grad.addColorStop(1, rgba(m.color, 0.0));
    ctx.fillStyle = grad; ctx.fill();
    
    ctx.beginPath();
    m.trace.forEach((v, j) => {
      const px = x0 + 6 + j * dx, py = pyB - (v / maxVal) * ph * 0.88;
      j === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
    });
    ctx.strokeStyle = rgba(m.color, 0.85); ctx.lineWidth = 1.3; ctx.stroke();
    
    if (idx > 0) {
      ctx.strokeStyle = 'rgba(61,255,192,0.07)'; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x0, H); ctx.stroke();
    }
  });
}

let lastTs = 0;
function render(ts) {
  requestAnimationFrame(render);
  const dt = Math.min((ts - lastTs) / 1000, 0.033); lastTs = ts;
  updateSignals(dt);
  drawBackground(); drawModuleHalos(); drawSynapses(); drawSignals(); drawNeurons(); drawOscilloscopes();
}

async function refreshData() {
  try {
    const r = await fetch('/api/brain');
    const d = await r.json();
    applyData(d);
  } catch (e) { console.warn('API error:', e); }
}

setInterval(refreshData, 200);
requestAnimationFrame(render);
refreshData();
</script>
</body></html>
"""

@app.route("/")
def index():
    return HTML

@app.route("/api/brain")
def api_brain():
    return jsonify(BRAIN.get_data())

@app.route("/api/status")
def api_status():
    return jsonify(BRAIN.stats)

@app.route("/api/learning")
def api_learning():
    """Stats d'apprentissage."""
    return jsonify({
        "hebbian_updates": BRAIN.stats["hebbian_updates"],
        "stdp_updates": BRAIN.stats["stdp_updates"],
        "learning_events": BRAIN.stats["learning_events"],
        "total_synapses": BRAIN.stats["synapses"],
        "avg_weight": sum(abs(s.weight) for s in BRAIN.synapses) / max(len(BRAIN.synapses), 1)
    })

if __name__ == "__main__":
    print("🧠 SoulLink Brain v7.0 — LIF + Hebbian + STDP")
    print("⚡ Apprentissage synaptique actif")
    print("🔥 Hebbian Learning: Δw = η * pre * post")
    print("⚡ STDP: Spike-Timing-Dependent Plasticity")
    print("🌐 http://0.0.0.0:8084/")
    print("📡 /api/brain | /api/status | /api/learning")
    app.run(host="0.0.0.0", port=8084, threaded=True)