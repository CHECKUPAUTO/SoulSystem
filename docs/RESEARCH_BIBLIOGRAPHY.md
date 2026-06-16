# Research Bibliography — Papers Useful to SoulSystem

Status: curated 2026-06-15 via web search. Companion to
`docs/RESEARCH_FRONTIER_2026.md` (the prioritized roadmap vs. OpenClaw /
Hermes-Agent) — this is the broader reading list, ~55 papers mapped to the
crate each one informs, with a one-line "how we'd apply it" note.

> Provenance note (honesty invariant): every arXiv ID below was returned by a
> web search on 2026-06-15; titles/IDs are reproduced as found. Verify the ID
> and version before citing formally. Foundational pre-2025 works are marked
> *(foundational)*.

---

## 1. Multi-provider routing & model selection
Crates: `avid-model-router`, `soullink-gateway/src/provider`, `soullink-moe`.

- **RouteLLM: Learning to Route LLMs with Preference Data** — arXiv:2406.18665 *(foundational)* — the strong/weak preference-trained router our `DifficultyModel` generalizes; baseline for `train_on_outcomes`.
- **UCCI: Calibrated Uncertainty for Cost-Optimal LLM Cascade Routing** — arXiv:2605.18796 — isotonic-calibrated per-query error → constrained cost-min threshold; directly upgrades `calibrate_threshold` and the uncertainty band.
- **Leveraging Uncertainty Estimation for Efficient LLM Routing** — arXiv:2502.11021 — confidence-driven escalation; the basis of our `uncertain` flag + `escalation_target`.
- **A Unified Approach to Routing and Cascading for LLMs** — arXiv:2410.10347 — theory unifying route-vs-cascade; informs combining `route_with_escalation` with best-of-N.
- **Dynamic Model Routing and Cascading: A Survey** — arXiv:2603.04445 — taxonomy for the router redesign; PILOT-style online+offline feedback.
- **A Survey on Inference Optimization Techniques for MoE** — arXiv:2412.14219 — model/system/hardware MoE optimization map for `soullink-moe`.
- **Routing-Free Mixture-of-Experts** — arXiv:2604.00801 — removes the router bottleneck; alternative for the MoE layer.
- **MELINOE: Fine-Tuning Enables Memory-Efficient MoE Inference** — arXiv:2602.11192 — memory-efficient expert serving for local fleets.

## 2. Agent memory
Crates: `soul-cognition` (`memory.rs`), `soullink-memory-hierarchy`, `soul-memory`.

- **Mem0: Production-Ready AI Agents with Scalable Long-Term Memory** — arXiv:2504.19413 — the ADD/UPDATE/DELETE/NOOP reconciliation we implemented in `reconcile_set`; graph variant next.
- **A-MEM: Agentic Memory for LLM Agents** — arXiv:2502.12110 — self-organizing links between records; backs `recall_associative`.
- **Hindsight is 20/20: Memory that Retains, Recalls, and Reflects** — arXiv:2512.12818 — the reflexive tier (retain/recall/reflect over the agent's own past).
- **Choosing How to Remember: Adaptive Memory Structures for LLM Agents** — arXiv:2602.14038 — picks the memory structure per task; informs tier selection in `CognitiveMemory`.
- **AI Meets Brain: Memory Systems from Cognitive Neuroscience to Autonomous Agents** — arXiv:2512.23343 — neuroscience→agent mapping validating the 5-tier design.
- **MemMachine: A Ground-Truth-Preserving Memory System** — arXiv:2604.04853 — provenance-preserving updates; aligns with our `Provenance` guard.
- **A Survey on the Security of Long-Term Memory in LLM Agents** — arXiv:2604.16548 — memory poisoning/extraction threats for the fact tiers.
- **Governing Evolving Memory in LLM Agents (SSGM)** — arXiv:2603.11768 — stability/safety governance for the eviction + reconciliation policy.

## 3. Self-improving skills & recursive self-improvement
Crates: `soul-skills`, `soul-rsi`, `soul-automodify`.

- **Voyager: An Open-Ended Embodied Agent with LLMs** — arXiv:2305.16291 *(foundational)* — the executable skill-library blueprint; `soul-skills` = library, `soul-rsi` = verifier.
- **SEVerA: Verified Synthesis of Self-Evolving Agents** — arXiv:2603.25111 — retain a synthesized skill only if it passes verification; exactly our `ValidatedSkillLibrary` gate.
- **RL for Self-Improving Agent with Skill Library (SAGE)** — arXiv:2512.17102 — GRPO over the skill library when a reward signal exists.
- **Darwin Gödel Machine: Open-Ended Evolution of Self-Improving Agents** — arXiv:2505.22954 — the empirical-fitness archive `soul-rsi` implements.
- **A Survey of Self-Evolving Agents (What/When/How/Where to Evolve)** — arXiv:2507.21046 — taxonomy positioning our gated approach.
- **Group-Evolving Agents: Open-Ended Self-Improvement via Experience Sharing** — arXiv:2602.04837 — cross-agent skill sharing for `soul-subagents`.
- **Agent Skills for LLMs: Architecture, Acquisition, Security** — arXiv:2602.12430 — skill lifecycle + security; informs `StructuralValidator`'s tool allow-list.
- **Reflexion: Language Agents with Verbal Reinforcement Learning** — arXiv:2303.11366 *(foundational)* — verbal reinforcement feeding the cognitive loop's reflect phase.

## 4. Verified multi-agent orchestration
Crates: `soullink-senate`, `soullink-orchestrator`, `soul-subagents`.

- **Multi-Agent Verification: Scaling Test-Time Compute with Multiple Verifiers** — arXiv:2502.20379 — the aspect-verifier panel `VerifiedAggregator` realizes (BoN-MAV beats self-consistency).
- **Verified Multi-Agent Orchestration: A Plan-Execute Framework** — arXiv:2603.11445 — DAG decomposition + coverage-gated replanning; our `needs_replanning` gate.
- **Multi-Agent LLM Orchestration Achieves Deterministic Decision Support** — arXiv:2511.15755 — zero-variance outputs for SLAs; deterministic verifiers.
- **Rethinking Optimal Verification Granularity for Test-Time Scaling** — arXiv:2505.11730 — how finely to verify; tunes verifier cost/coverage.
- **Trust but Verify! A Survey on Verification Design for Test-Time Scaling** — arXiv:2508.16665 — verifier-design taxonomy for the senate.
- **CTTS: Collective Test-Time Scaling** — arXiv:2508.03333 — collective scaling pattern complementing the agreement signal.
- **AgentOrchestra: A Hierarchical Multi-Agent Framework** — arXiv:2506.12508 — planner-as-orchestrator pattern for `soullink-orchestrator`.
- **From Agent Loops to Structured Graphs (Scheduler-Theoretic)** — arXiv:2604.11378 — plan/execute separation as a scheduled graph.
- **Routine: A Structural Planning Framework for Enterprise Agents** — arXiv:2507.14447 — plan-then-act structure for reliability.

## 5. Reasoning & test-time compute
Crates: `soullink-reasoning`, `soullink-inference`.

- **ReAct: Synergizing Reasoning and Acting in Language Models** — arXiv:2210.03629 *(foundational)* — the observe→think→act loop of the autonomous entity.
- **Atom of Thoughts for Markov LLM Test-Time Scaling** — arXiv:2502.12018 — decompose reasoning into atomic units; cheaper deep reasoning.
- **Thought Calibration: Efficient and Confident Test-Time Scaling** — arXiv:2505.18404 — decide *when* to stop thinking; pairs with the agreement signal.
- **Forest-of-Thought: Scaling Test-Time Compute** — arXiv:2412.09078 — multiple reasoning trees + sparse activation.
- **The Art of Scaling Test-Time Compute for LLMs** — arXiv:2512.02008 — practical scaling laws for the inference budget.
- **Chain-in-Tree: Back to Sequential Reasoning in LLM Tree Search** — arXiv:2509.25835 — efficiency of tree search vs. sequential reasoning.

## 6. Tool use & agentic RL
Crates: `soul_tools`, `soul-agent-core`, `soul-rsi`.

- **Toolformer: Language Models Can Teach Themselves to Use Tools** — arXiv:2302.04761 *(foundational)* — self-supervised API-call learning for `soul_tools`.
- **RLVR Implicitly Incentivizes Correct Reasoning in Base LLMs** — arXiv:2506.14245 — verifiable-reward RL; our empirical gate is the verifier.
- **VerlTool: Holistic Agentic RL with Tool Use** — arXiv:2509.01055 — tool-integrated RL training stack.
- **The Landscape of Agentic RL for LLMs: A Survey** — arXiv:2509.02547 — map of agentic-RL methods for the autonomy loop.
- **Direct Reasoning Optimization (rubric-gated constraints)** — arXiv:2506.13351 — rubric-gated RL; aligns with our `RubricVerifier`.

## 7. Retrieval-augmented generation
Crates: `soullink-rag`, `soul-memory`.

- **Agentic Retrieval-Augmented Generation: A Survey** — arXiv:2501.09136 — agentic RAG patterns (reflection/planning/tool-use) for `soullink-rag`.
- **A-RAG: Scaling Agentic RAG via Hierarchical Retrieval Interfaces** — arXiv:2602.03442 — hierarchical retrieval for large corpora.
- **TreePS-RAG: Tree-based Process Supervision for RL in Agentic RAG** — arXiv:2601.06922 — process-supervised RAG retrieval.

## 8. Context management & long-horizon efficiency
Crates: `soul-compaction`, context plumbing in `soul_llm`.

- **Acon: Optimizing Context Compression for Long-horizon LLM Agents** — arXiv:2510.00615 — doc/dialogue/KV compression taxonomy for `soul-compaction`.
- **Beyond Compaction: Structured Context Eviction for Long-Horizon Agents** — arXiv:2606.11213 — branch/return sub-contexts; structured eviction like our bounded tiers.
- **Still: Amortized KV Cache Compaction in a Single Forward Pass** — arXiv:2606.07878 — 8×–200× KV compaction.
- **Efficient On-Device Agents via Adaptive Context Management** — arXiv:2511.03728 — inter/intra-session context for local deployment.

## 9. Honesty, factuality & calibration
Crates: `soul-cognition` (provenance), `soul-critique`.

- **Mitigating LLM Hallucination via Behaviorally Calibrated RL** — arXiv:2512.19920 — train models to abstain when unsure; epistemic honesty (our invariant #1).
- **Large Language Models Hallucination: A Comprehensive Survey** — arXiv:2510.06265 — detection taxonomy (retrieval/uncertainty/self-consistency).
- **Logical Consistency as a Bridge for Hallucination Detection (LaaB)** — arXiv:2605.03971 — neural↔symbolic consistency for the critique layer.

## 10. Security, sandboxing & prompt injection
Crates: `soul-sandbox`, BoundSystem, `src/code_signing`, `soullink-gate`.

- **Architecting Resilient LLM Agents: Secure Plan-then-Execute** — arXiv:2509.08646 — strong isolation for code execution (bubblewrap/seccomp rationale).
- **Securing AI Agent Execution** — arXiv:2510.21236 — manifest permissions + runtime consent + network allow-lists.
- **Fault-Tolerant Sandboxing for AI Coding Agents (Transactional)** — arXiv:2512.12806 — atomic, rollback-able agent actions.
- **Agentic AI Security: Threats, Defenses, Evaluation** — arXiv:2510.23883 — emulator+evaluator dual-agent isolation.
- **VIGIL: Defending Agents Against Tool Stream Injection (Verify-Before-Commit)** — arXiv:2601.05755 — verify tool outputs before acting; realized by `soullink-gate::injection` (signature/canary/encoding-evasion ensemble + spotlighting) and `ApprovalGate::screen_tool_output`, the inbound dual of the outbound approval gate.
- **AttriGuard: Defeating Indirect Prompt Injection via Causal Attribution** — arXiv:2603.10749 — attribute tool-invocation causes; IPI defense.
- **Quantifying Frontier LLM Capabilities for Container Sandbox Escape** — arXiv:2603.02277 — escape risks to harden the sandbox against.

## 11. Agent protocols & messaging channels
Crates: `soul-mcp`, `soullink-gateway`, `soul-bridge`.

- **A Survey of Agent Interoperability Protocols (MCP/ACP/A2A/ANP)** — arXiv:2505.02279 — protocol landscape for `soul-mcp` and the gateway.
- **MCP Tool Descriptions Are Smelly! (Augmented Descriptions)** — arXiv:2602.14878 — better tool descriptions → higher tool-call accuracy.
- **Coral Protocol: Open Infrastructure Connecting the Internet of Agents** — arXiv:2505.00749 — agent-to-agent infrastructure ideas for `soul-bridge`.
- **Permission Manifests for Web Agents** — arXiv:2601.02371 — per-capability permission manifests; informs `soullink-gate`.

## 12. Web exploration & API cloning
Crates: `avid-core`, `avid-cortex`, `avid-scout`, `avid-tokenjuice`, `soul-browser`, `soul-webfetch`.

- **WebLists / BardeenAgent: Structured Extraction from Interactive Websites** — arXiv:2504.12682 — record an extraction as a replayable program (CSS selectors); directly relevant to `avid-scout`.
- **ScrapeGraphAI-100k: Dataset for Schema-Constrained LLM Extraction** — arXiv:2602.15189 — schema-constrained web extraction at scale.
- **WebChoreArena: Evaluating Web Browsing Agents on Tedious Tasks** — arXiv:2506.01952 — eval harness for the browser agents.
- **WorldGUI: Interactive Benchmark for Desktop GUI Automation** — arXiv:2502.08047 — GUI-automation benchmark for `soul-browser`.

## 13. Distillation & on-device efficiency
Crates: `scirust-*`, local-model routing.

- **Knowledge & Dataset Distillation of LLMs: Trends & Directions (survey)** — arXiv:2504.14772 — compress teacher capability into deployable students.
- **A Survey of On-Policy Distillation for LLMs** — arXiv:2604.00626 — on-policy distillation for the local fleet.
- **MiniLLM: On-Policy Distillation of LLMs** — arXiv:2306.08543 *(foundational)* — reverse-KL distillation method.

## 14. Physics / dynamics core
Crates: `soullink-core` (HNN engine, Verlet symplectic), `scirust-core`, `scirust-autodiff`.

- **Mastering High-Dimensional Dynamics with Hamiltonian Neural Networks** — arXiv:2008.04214 *(foundational)* — HNN energy-conserving dynamics underpinning the mesh.
- **Kolmogorov–Arnold Representation for Symplectic Learning (KAR-HNN)** — arXiv:2508.19410 — KAN-based HNN improving long-horizon stability.
- **Frequency-Separable Hamiltonian Neural Network (Multi-Timescale)** — arXiv:2603.06354 — stiff/multi-timescale dynamics while conserving energy.
- **ATLAS-NN: Adaptive Transfer Learnable Symplectic-aware NN** — arXiv:2606.04447 — long-time Hamiltonian dynamics with transfer.

---

## How to use this list

1. The roadmap in `docs/RESEARCH_FRONTIER_2026.md` is the *prioritized* subset
   (routing → skills → memory → verified orchestration). This file is the
   *breadth* reference.
2. The through-line stays the same: route every borrowed technique through
   `soul-rsi`'s empirical gate and `soul-cognition`'s provenance/permission
   layer, so we adopt the field's best ideas without inheriting their failure
   mode (silent regression / fabricated confidence).
3. Before relying on any 2026 entry, confirm the arXiv ID/version — these were
   gathered by automated search.
