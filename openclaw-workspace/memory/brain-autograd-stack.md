# Brain Project — Autograd Engines

> **Tuile d'info pour Claude/claude-code/codex** — Ce fichier documente les moteurs autograd disponibles sur le serveur pour le projet Brain.

---

## Stack AutoGrad Installé

### 1. PyTorch — `torch` (MOTEUR PRINCIPAL)
- **Version :** 2.6.0+cu124 (CUDA 12.4)
- **GPU :** NVIDIA RTX 4060 (8 Go VRAM)
- **Capacités :**
  - `torch.autograd` — backward, grad, grad_fn
  - `requires_grad` pour tensors追踪
  - GPU acceleration native
  - `torch.jit`, `torch.nn`, `torch.optim`
  - `torch.distributed` (multi-GPU)
- **Use case :** Training loops, neural nets, research

### 2. JAX — `jax` + `jaxlib`
- **Version :** JAX 0.9.2
- **GPU :** NVIDIA RTX 4060 (CUDA 12.4)
- **Capacités :**
  - `jax.grad` — autograd fonctionnel
  - `jax.jit` — compilation XLA
  - `jax.vmap` — vectorization
  - `jax.lax` — low-level ops
  - `jax.numpy` — API NumPy compatible
- **Add-ons installés :**
  - `flax 0.12.6` — neural network library (LinAlg, optimizers)
  - `optax 0.2.8` — gradient-based optimization
- **Use case :** Research, TPU-style code, functional ML

### 3. Objax — `objax`
- **Version :** 1.8.0
- **GPU :** NVIDIA RTX 4060
- **Capacités :**
  - Autograd for research (Google Brain)
  - Modular NN primitives
  - `objax.functional`, `objax.nn`, `objax.optimizer`
- **Use case :** Research, diffusion models, custom architectures

### 4. PaddlePaddle — `paddle`
- **Version :** 3.3.1 (CPU + CUDA via paddle)
- **GPU :** NVIDIA RTX 4060
- **Capacités :**
  - `paddle.autograd` — backward engine
  - `paddle.nn` — layers, loss, optimizers
  - `paddle.incubate` — advanced features
  - High-level API style (similar to PyTorch)
- **Use case :** Production, enterprise, Chinese LLMs

---

## Incompatibles (Python 3.13.5)

| Package | Raison |
|---------|--------|
| `haiku` (Google) | Pas de wheel Python 3.13 |
| `paddlepaddle-gpu` | Pas de build CUDA Python 3.13 |
| `mindspore` (Huawei) | Pas de wheel Python 3.13 |
| `chainer` | Deprecated, plus de support Python 3.13 |

---

## Mémo Usage

```python
# PyTorch (principal)
import torch
x = torch.tensor([2., 3.], requires_grad=True)
y = x ** 2
z = y.sum()
z.backward()

# JAX (fonctionnel)
import jax
import jax.numpy as jnp
from jax import grad
def f(x): return jnp.sum(x ** 2)
grad(f)(jnp.array([2., 3.]))

# Objax
import objax
# objax-style grads

# PaddlePaddle
import paddle
paddle.set.device('gpu')
x = paddle.to_tensor([2., 3.], stop_gradient=False)
```

---

## Notes Projet

- **Serveur :** Debian + NVMe + RTX 4060 8 Go
- **Python :** 3.13.5 (pas de venv — installation système)
- **CUDA :** 12.4 / Driver 595.45
- **Modèles Ollama :** 38 modèles dispo (cloud + local)
- **But projet Brain :** Architecture auto-évolutive avec mémoire cognitive

---
_Mis à jour : 2026-04-08_
