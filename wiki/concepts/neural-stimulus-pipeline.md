# Neural Mesh Stimulus Pipeline

**Summary:** Design for wiring real-world events into the neural mesh. Currently the mesh is structurally alive but functionally dormant (Hz=0.0) — no stimulus enters the system.

**Full reference:** [evolution/references/neural_mesh_stimulus_pipeline.md](../../evolution/references/neural_mesh_stimulus_pipeline.md)

**Key findings:**
- Orchestrator shows 0 queries processed — mesh receives ZERO input
- Proposed: Gateway plugin POSTing conversation events to /api/mesh/stimulus
- Cross-node resonance with Hebbian synaptic plasticity
- Turbulence cascade: 6×6 weight matrix for cross-organ influence
- Sleep consolidation when all organs in DeepBasin >30 min
- Node binary unification would save ~55M RAM
- Gateway WS 1006: env var now correct (18890), may just need restart

**Created:** 2026-04-14