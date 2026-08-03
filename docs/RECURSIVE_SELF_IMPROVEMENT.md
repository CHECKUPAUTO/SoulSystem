# Recursive Self-Improvement in SoulSystem

> *"Let an ultraintelligent machine be defined as a machine that can far surpass
> all the intellectual activities of any man however clever. Since the design of
> machines is one of these intellectual activities, an ultraintelligent machine
> could design even better machines."* — I. J. Good, 1965

This document does three things:

1. **Audits** the self-improvement machinery already present in SoulSystem.
2. **Maps** the relevant scientific literature (50 papers, below) onto concrete
   components of the repo.
3. **Specifies** the missing piece that turns a collection of autonomous
   behaviours into a genuine *closed* recursive-self-improvement (RSI) loop —
   now implemented as the [`soul_rsi`](../soul-rsi) crate.

Anthropic's *"When AI builds itself"* essay does not describe a single new
technique; it describes the **convergence** of 30–60 years of research that
becomes practical once an LLM can manipulate code, plan experiments, and
interpret results. SoulSystem already contains most of those lines of research
in working form. What it lacked was the empirically-gated loop that ties
*propose → build → test → keep-only-if-better → archive → repeat* together. That
is what we add.

---

## 1. Levels of self-improvement (and where SoulSystem sits)

| Level | Description | SoulSystem status |
|-------|-------------|-------------------|
| 1. Software self-improvement | AI writes/improves code in its own environment | ✅ `openevolve`, `soulsystem-evolution`, `soul_tools` |
| 2. Architecture self-improvement | AI proposes better model architectures | 🟡 `soullink-brain` HNN, `forge-core` (generic), NAS-style search not yet wired |
| 3. Learning self-improvement | AI changes its own training/objectives/optimizers | 🟡 `soullink-trainer`, `soul-critique` (Reflexion), `soul_planner` |
| 4. Full RSI | AI designs, trains, and deploys its successor unattended | ⛔ not reached — and deliberately gated by the build+test validation wall in `soul_rsi` |

Anthropic is explicit that **Level 4 is not here**. SoulSystem's design follows
the same posture: the new loop can only *propose* and *validate* changes in an
isolated sandbox; promoting a change to the live tree is a separate, gated,
audited step.

---

## 2. Audit — what already exists

| Capability | Component | Status |
|------------|-----------|--------|
| LLM-guided code evolution (AlphaEvolve-style) | `openevolve/` | **wired** — CLI `evolve` + server on :8460 |
| Open-ended agent evolution loop | `soulsystem-evolution/` | **wired** — `run_forever()` cycle |
| Generic multi-objective evolutionary engine (NSGA-II) | `forges/forge-core/` | **library** — `Domain` trait, `Engine::run`, `Checkpoint` |
| ReAct agent w/ memory distillation + skill crystallization | `soul-agent-core/` | **wired** — used by `soul-daemon` |
| Self-critique / Reflexion | `soul-critique/` | **wired** — `quick_critique` in agent loop |
| Plan evaluation + decision | `soul_planner/` (`CognitiveLoop`) | **wired** |
| Sandboxed execution (bubblewrap + seccomp) | `soul-sandbox/`, `bound-system/` | **partial** |
| Source self-modification w/ backup & rollback | `soul-automodify/` | **was dead code** — now driven by `soul_rsi` |
| Autonomous observe→plan→act→evaluate→decide loop | `src/autonomous_loop.rs` | **wired** |

**The gap:** nothing closed the loop *propose a source change → build & test it in
isolation → keep it only if it provably improves → archive the survivor →
branch again*. `soul-automodify` could patch files but no caller used it, and no
component subjected a proposed change to an empirical build+test gate before
accepting it. That is exactly the loop STOP and the Darwin Gödel Machine
formalise, and it is what `soul_rsi` now provides.

---

## 3. The closed loop (`soul_rsi`)

```text
        ┌──────────────────────────────────────────────────────────┐
        │  Archive  (open-ended population of accepted variants)    │
        │     • root = unmodified codebase                          │
        │     • every kept variant is a "stepping stone"            │
        └───────────────┬──────────────────────────────────────────┘
                        │ select_parent  (quality × novelty)
                        ▼
                 Proposer.propose   ← STOP "improver" (LLM or heuristic)
                        │              fed: goal, parent fitness, past rejections
                        ▼
        snapshot the live tree → apply patch in the COPY  (soul-automodify)
                        │
                        ▼
                 Evaluator.evaluate   → Fitness { compiles, tests_passed,
                        │                          tests_failed, score }
                        ▼
        is_better_than(parent)  AND  passes the all-green gate?
              │ yes                         │ no
              ▼                             ▼
        insert into Archive           discard + remember the rationale
              │                          (verbal reinforcement, Reflexion)
              └──────────────► (repeat) ◄──┘
```

Key properties, each traceable to a paper:

- **Empirical gate first.** `Fitness` orders lexicographically:
  `(compiles, −tests_failed, tests_passed, score)`. A change that breaks the
  build can *never* dominate one that builds, no matter what score the proposer
  claims. → *Gödel Machine* (provable-benefit before self-modification).
- **Open-ended archive.** Every validated variant is kept and may be branched
  from, not just the current best. → *Darwin Gödel Machine* + *open-endedness*.
- **Swappable improver.** The proposer is just a trait (`CodeModel`); the loop
  never trusts its self-assessment. → *STOP*.
- **Verbal reinforcement.** Rejected rationales are fed back to the proposer.
  → *Reflexion*.
- **Reproducibility.** A `SplitMix64` seed replays the exact proposal/selection
  sequence, so every accepted improvement is auditable.
- **Safety boundary.** Evaluation always runs on a throwaway copy; the live tree
  is only touched by an explicit, all-green-gated `promote_to_live` call, and
  every edit is backed up via `soul-automodify`.

### Wiring it to the rest of SoulSystem

- The `Proposer` LLM adapter (`LlmProposer`) accepts any `CodeModel`; the
  production binding wraps `soul_llm`/Ollama.
- The `CargoEvaluator` is the validation wall; for model/architecture search it
  can be swapped for a benchmark-scoring evaluator (Level 2), reusing
  `forge-core`'s NSGA-II selection.
- Accepted variants can feed `soul-agent-core`'s skill crystallization and the
  `openevolve` program database, unifying the two evolutionary subsystems.

---

## 4. Bibliography (mapped to SoulSystem)

Legend of the **SoulSystem** column: which component embodies the idea.

### Foundations of self-improving machines
1. I. J. Good (1965) — *Speculations Concerning the First Ultraintelligent Machine.* — concept of the intelligence explosion. → `soul_rsi` (the loop's premise). <https://www.cs.virginia.edu/~robins/Good_IJ_1965.pdf>
2. J. Schmidhuber (1987) — *Evolutionary Principles in Self-Referential Learning* (diploma thesis; "learning to learn"). → meta-learning premise. <https://people.idsia.ch/~juergen/>
3. J. Schmidhuber (1997) — *Discovering Neural Nets with Low Kolmogorov Complexity.* → `soullink-brain` HNN priors. <https://people.idsia.ch/~juergen/>
4. J. Schmidhuber (2007) — *Gödel Machines: Fully Self-Referential Optimal Universal Self-Improvers.* → `soul_rsi` provable-benefit gate. <https://arxiv.org/abs/cs/0309048>
5. R. Yampolskiy (2015) — *From Seed AI to Technological Singularity via Recursively Self-Improving Software.* → `soul_rsi` taxonomy. <https://arxiv.org/abs/1502.06512>
6. Nivel et al. (2013) — *Bounded Recursive Self-Improvement.* → `src/autonomous_loop.rs` bounded loop. <https://arxiv.org/abs/1312.6764>

### The direct self-improving loop
7. Zelikman et al. (2023) — *Self-Taught Optimizer (STOP): Recursively Self-Improving Code Generation.* → `soul_rsi::LlmProposer` (the improver). <https://arxiv.org/abs/2310.02304>
8. Yin et al. (2024) — *Gödel Agent: A Self-Referential Framework for Recursive Self-Improvement.* → `soul_rsi` engine. <https://arxiv.org/abs/2410.04444>
9. Zhang et al. (2025) — *Darwin Gödel Machine: Open-Ended Evolution of Self-Improving Agents.* → `soul_rsi::Archive` (stepping stones). <https://arxiv.org/abs/2505.22954>
10. Lu et al. (2024) — *The AI Scientist: Towards Fully Automated Open-Ended Scientific Discovery.* → `openevolve` + `synergie`. <https://arxiv.org/abs/2408.06292>

### Neural Architecture Search / AutoML (AI designs AI)
11. Zoph & Le (2017) — *Neural Architecture Search with Reinforcement Learning.* → `forge-core` (search engine). <https://arxiv.org/abs/1611.01578>
12. Pham et al. (2018) — *Efficient NAS via Parameter Sharing (ENAS).* <https://arxiv.org/abs/1802.03268>
13. Liu, Simonyan, Yang (2019) — *DARTS: Differentiable Architecture Search.* <https://arxiv.org/abs/1806.09055>
14. Real et al. (2019) — *Regularized Evolution for Image Classifier Architecture Search (AmoebaNet).* → `forge-core` evolutionary selection. <https://arxiv.org/abs/1802.01548>
15. Elsken, Metzen, Hutter (2019) — *Neural Architecture Search: A Survey.* <https://arxiv.org/abs/1808.05377>
16. Baker et al. (2017) — *Designing Neural Network Architectures using Reinforcement Learning (MetaQNN).* <https://arxiv.org/abs/1611.02167>
17. Hutter, Kotthoff, Vanschoren (2019) — *Automated Machine Learning: Methods, Systems, Challenges.* <https://www.automl.org/book/>
18. So, Liang, Le (2019) — *The Evolved Transformer.* → `soullink-brain` architecture evolution. <https://arxiv.org/abs/1901.11117>
19. Tan & Le (2019) — *EfficientNet: Rethinking Model Scaling for CNNs.* <https://arxiv.org/abs/1905.11946>
20. Cai, Zhu, Han (2019) — *ProxylessNAS: Direct NAS on Target Task and Hardware.* → `avid-model-router` HW-aware routing. <https://arxiv.org/abs/1812.00332>

### LLM as optimizer / meta-programmer
21. Yang et al. (2023) — *Large Language Models as Optimizers (OPRO).* → `soul_rsi::LlmProposer`. <https://arxiv.org/abs/2309.03409>
22. Zhou et al. (2023) — *Large Language Models Are Human-Level Prompt Engineers (APE).* → `soulsystem-gepa`. <https://arxiv.org/abs/2211.01910>
23. Romera-Paredes et al. (2024) — *FunSearch: Mathematical discoveries from program search with LLMs (Nature).* → `openevolve`. <https://www.nature.com/articles/s41586-023-06924-6>
24. Novikov et al. (2025) — *AlphaEvolve: A coding agent for scientific and algorithmic discovery.* → `openevolve` (direct analogue). <https://arxiv.org/abs/2506.13131>
25. Lehman et al. (2022) — *Evolution through Large Models (ELM).* → `soulsystem-evolution`. <https://arxiv.org/abs/2206.08896>

### Agents that plan / reflect / use tools
26. Yao et al. (2022) — *ReAct: Synergizing Reasoning and Acting in Language Models.* → `soul-agent-core` (ReAct loop). <https://arxiv.org/abs/2210.03629>
27. Shinn et al. (2023) — *Reflexion: Language Agents with Verbal Reinforcement Learning.* → `soul-critique::ReflexionLoop` + `soul_rsi` rejection memory. <https://arxiv.org/abs/2303.11366>
28. Yao et al. (2023) — *Tree of Thoughts.* → `soullink-reasoning::ThoughtTree`. <https://arxiv.org/abs/2305.10601>
29. Wang et al. (2023) — *Self-Consistency Improves Chain-of-Thought Reasoning.* → `soullink-senate` (voting). <https://arxiv.org/abs/2203.11171>
30. Schick et al. (2023) — *Toolformer: Language Models Can Teach Themselves to Use Tools.* → `soul_tools`. <https://arxiv.org/abs/2302.04761>
31. Wang et al. (2023) — *Voyager: An Open-Ended Embodied Agent with LLMs* (skill library). → `soul-agent-core` skill crystallization. <https://arxiv.org/abs/2305.16291>
32. Park et al. (2023) — *Generative Agents: Interactive Simulacra of Human Behavior.* → `soulsystem-multiagent`. <https://arxiv.org/abs/2304.03442>
33. Wei et al. (2022) — *Chain-of-Thought Prompting Elicits Reasoning.* <https://arxiv.org/abs/2201.11903>

### Meta-learning ("learning to learn")
34. Finn, Abbeel, Levine (2017) — *Model-Agnostic Meta-Learning (MAML).* → `soullink-trainer` fast adaptation. <https://arxiv.org/abs/1703.03400>
35. Hospedales et al. (2021) — *Meta-Learning in Neural Networks: A Survey.* <https://arxiv.org/abs/2004.05439>
36. Andrychowicz et al. (2016) — *Learning to Learn by Gradient Descent by Gradient Descent.* <https://arxiv.org/abs/1606.04474>
37. Wang et al. (2016) — *Learning to Reinforcement Learn (RL²).* <https://arxiv.org/abs/1611.05763>

### Self-play / reinforcement learning foundations
38. Silver et al. (2017) — *Mastering Chess and Shogi by Self-Play (AlphaZero).* → `soullink-senate`/self-play idea. <https://arxiv.org/abs/1712.01815>
39. Silver et al. (2016) — *Mastering the Game of Go with Deep Neural Networks and Tree Search.* <https://www.nature.com/articles/nature16961>
40. Mnih et al. (2015) — *Human-level control through deep reinforcement learning (DQN).* <https://www.nature.com/articles/nature14236>
41. Sutton & Barto (2018) — *Reinforcement Learning: An Introduction (2e).* <http://incompleteideas.net/book/the-book.html>
42. Christiano et al. (2017) — *Deep RL from Human Preferences (RLHF).* → `soul-critique` reward shaping. <https://arxiv.org/abs/1706.03741>

### Evolutionary computation / open-endedness
43. Holland (1992) — *Adaptation in Natural and Artificial Systems.* → `forge-core` GA. <https://mitpress.mit.edu/9780262581110/>
44. Stanley & Lehman (2015) — *Why Greatness Cannot Be Planned: The Myth of the Objective.* → `soul_rsi::Archive` novelty bonus. <https://link.springer.com/book/10.1007/978-3-319-15524-1>
45. Lehman et al. (2020) — *The Surprising Creativity of Digital Evolution.* → `soulsystem-evolution` (sandbox guards). <https://arxiv.org/abs/1803.03453>
46. Mouret & Clune (2015) — *Illuminating search spaces by mapping elites (MAP-Elites).* → `soul_rsi` quality-diversity selection. <https://arxiv.org/abs/1504.04909>
47. Stanley & Miikkulainen (2002) — *Evolving Neural Networks through Augmenting Topologies (NEAT).* <https://doi.org/10.1162/106365602320169811>

### Core architectures & training (the substrate)
48. Vaswani et al. (2017) — *Attention Is All You Need.* → `soullink-brain` / `scirust-core` transformer. <https://arxiv.org/abs/1706.03762>
49. Brown et al. (2020) — *Language Models are Few-Shot Learners (GPT-3).* <https://arxiv.org/abs/2005.14165>
50. R. Sutton (2019) — *The Bitter Lesson.* → design principle: prefer search + learning at scale. <http://www.incompleteideas.net/IncIdeas/BitterLesson.html>

### Safety / control of self-improvement (cross-cutting)
- Amodei et al. (2016) — *Concrete Problems in AI Safety.* → `soul_rsi` build/test gate + sandbox + `bound-system`. <https://arxiv.org/abs/1606.06565>
- Bostrom (2014) — *Superintelligence: Paths, Dangers, Strategies.* <https://global.oup.com/academic/product/superintelligence-9780199678112>
- Russell (2019) — *Human Compatible: AI and the Problem of Control.* <https://www.penguinrandomhouse.com/books/566677/human-compatible-by-stuart-russell/>

---

## 5. Roadmap to "industrial level"

1. **Wire the production proposer** — bind `LlmProposer` to `soul_llm` and run
   `soul_rsi` under `soul-daemon` with a strict path allow-list (start with
   leaf utility crates).
2. **Persist the archive** — store `Archive` JSON next to `openevolve`'s program
   DB so the two evolutionary subsystems share stepping stones.
3. **Promote with human-in-the-loop** — `promote_to_live` only on all-green +
   reviewer approval; emit each promotion to the audit log (`audit_log.rs`).
4. **Lift to Level 2** — swap `CargoEvaluator` for a benchmark evaluator over
   `soullink-brain` HNN configs and reuse `forge-core` NSGA-II for multi-objective
   (speed × accuracy × energy) architecture search.
5. **Close the meta-loop (STOP)** — let the proposer improve *itself* (the
   prompt / selection policy), measured by downstream archive growth.

The guardrail never moves: **a change is only ever kept if a fresh build and the
test suite say it is better.** That single invariant is what separates
disciplined recursive self-improvement from wishful thinking.
