# PROMPT — Brain Project AutoGrad Stack

---

## Contexte à copier/coller

Voici la stack Autograd disponible sur le serveur pour le projet Brain :

### Ce qui est installé

| Moteur | Version | GPU | Notes |
|--------|---------|-----|-------|
| **PyTorch** | 2.6.0+cu124 | ✅ RTX 4060 | Moteur principal, backward(), grad |
| **JAX** | 0.9.2 | ✅ RTX 4060 | grad() fonctionnel + XLA JIT |
| **Flax** | 0.12.6 | ✅ | NN library pour JAX |
| **Optax** | 0.2.8 | ✅ | Optimizers pour JAX |
| **Objax** | 1.8.0 | ✅ | Recherche, Google Brain |
| **PaddlePaddle** | 3.3.1 | ✅ CPU+GPU | API haut niveau, production |

### Mémo d'usage rapide

```python
# PyTorch — principal
import torch
x = torch.tensor([2., 3.], requires_grad=True)
y = x ** 2
z = y.sum()
z.backward()  # → x.grad = [4., 6.]

# JAX — fonctionnel, JIT
import jax
import jax.numpy as jnp
from jax import grad
def f(x): return jnp.sum(x ** 2)
grad(f)(jnp.array([2., 3.]))  # → [4., 6.]

# PaddlePaddle
import paddle
x = paddle.to_tensor([2., 3.], stop_gradient=False)
y = x ** 2
z = y.sum()
z.backward()  # GPU ready

# Objax
import objax
```

### Limitations
- `haiku`, `mindspore`, `chainer` — non disponibles (Python 3.13)
- `paddlepaddle-gpu` — pas de build CUDA Python 3.13 (CPU only, fallback GPU)

### Fichiers de référence
- Doc complète : `memory/brain-autograd-stack.md`
- Env : Python 3.13.5 / CUDA 12.4 / RTX 4060 8 Go

---

## Comment l'utiliser

Copie le bloc "Contexte à copier/coller" ci-dessus dans tes prompts Claude Code / Codex quand tu veux qu'il utilise un de ces moteurs pour le projet Brain.
