# Karpathy Gists — Analyse

_Concept page — ce qu'on retire des gists de Karpathy pour nos projets._

## Source
https://gist.github.com/karpathy

## Gists analysés

### 1. microgpt (2026-02-11)
- **URL**: https://gist.github.com/karpathy/8627fe009c40f57531cb18360106ce95
- **Contenu**: GPT complet en Python pur, 0 dépendance (pas même numpy). Autograd custom (classe Value), tokenizer char-level, transformer multi-head attention, Adam optimizer.
- **Intérêt pour nous**: 
  - Pattern d'autograd minimal → pourrait inspirer un moteur de différenciation pour SoulLink
  - Démontre que l'essentiel d'un GPT tient en ~200 lignes
  - La classe `Value` avec `__slots__` est un pattern d'optimisation mémoire applicable

### 2. build-microgpt (2026-02-13)
- **URL**: https://gist.github.com/karpathy/561ac2de12a47cc06a23691e1be9543a
- **Contenu**: Version progressive de microgpt — construit couche par couche (5 étapes : dataset → autograd → MLP → transformer → Adam)
- **Intérêt**: Approche pédagogique par "couches d'oignon" — applicable pour documenter une architecture complexe (ex: SoulLink V13)

### 3. pg-pong.py (2016)
- **URL**: https://gist.github.com/karpathy/a4166c7fe253700972fcbc77e4ea32c5
- **Contenu**: Agent Pong ATARI avec Policy Gradients depuis pixels bruts. 130 lignes de Python/numpy. Réseau 2 couches (200 neurones cachés), RMSProp, discount rewards.
- **Intérêt**:
  - **Pattern RL directement applicable** à SoulLink pour le reinforcement learning des nœuds
  - `discount_rewards()` → pattern de discount temporel applicable à notre TurbulenceEngine
  - Architecture minimale RL : forward → sample action → accumulate gradients → update
  - Le concept "fake label" (y - aprob) comme signal d'apprentissage est élégant

### 4. batched LSTM (2015)
- **URL**: https://gist.github.com/karpathy/587454dc0146a6ae21fc
- **Contenu**: LSTM vectorisé batché en numpy. Forward + backward pass. Fancy forget bias init (défaut=3).
- **Intérêt**:
  - Pattern de vectorisation batchée numpy → directement applicable si on remplace des boucles Python dans SoulLink V12
  - Le `fancy_forget_bias_init` positif pour encourager l'oubli → analogue à notre paramétrage d'attracteurs
  - Architecture IFOG (Input, Forget, Output, Gate) → pourrait structurer les gates neuronales de SoulLink

### 5. min-char-rnn (2015)
- **URL**: https://gist.github.com/karpathy/d4dee566867f8291f086
- **Contenu**: RNN char-level minimal en numpy. Forward + backward + sampling. ~100 lignes.
- **Intérêt**: 
  - Le pattern BPTT (Backprop Through Time) minimal → base théorique pour tout réseau récurrent
  - Architecture Wxh/Whh/Why → la matrice la plus simple qui apprend des séquences temporelles

### 6. llm-wiki (2026-04-04) ✅ Déjà ingéré
- Voir [llm-wiki-pattern](llm-wiki-pattern.md)

## Synthèse — Ce qui nous sert

| Pattern | Source | Application SoulLink/OpenEvolve |
|---------|--------|-------------------------------|
| Autograd minimal (Value) | microgpt | Moteur de différentiation custom |
| RL Policy Gradient | pg-pong | Reinforcement des nœuds, learning par récompense |
| LSTM vectorisé batché | batched LSTM | Remplacement boucles Python → ops vectorisées |
| Discount temporel | pg-pong | TurbulenceEngine, pondération des événements récents |
| BPTT minimal | min-char-rnn | Base théorique réseaux récurrents |
| Construction par couches | build-microgpt | Documentation pédagogique architecture |

## Voir aussi
- [soullink](../entities/soullink.md)
- [openevolve](../entities/openevolve.md)
- [llm-wiki-pattern](llm-wiki-pattern.md)