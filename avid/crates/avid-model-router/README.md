# avid-model-router

Model routing for SoulSystem / AVID. Two layers:

1. **Capability router** ([`ModelRouter`]) — a heuristic floor: pick a model that
   has all the required capabilities, ordered by priority and latency, with a
   fallback chain.
2. **Learned cost-aware router** ([`CostAwareRouter`]) — predicts how strong a
   model a query actually needs and routes to the **cheapest model that clears
   that bar**. This is the [RouteLLM](https://arxiv.org/abs/2406.18665)
   strong/weak deferral idea generalized to an N-model cascade, with
   cost-aware selection and uncertainty-based escalation. See
   `docs/RESEARCH_FRONTIER_2026.md` §2.

The learned layer is the one that lets SoulSystem beat OpenClaw's heuristic
provider selection on cost at equal quality.

## How the learned router decides

```
query ──▶ QueryFeatures ──▶ DifficultyModel ──▶ difficulty d ∈ [0,1]
                                                      │
                  cost_aversion lowers the bar  ◀─────┤
                                                      ▼
        candidates with required capabilities ──▶ quality bar = lerp(min,max,d)
                                                      ▼
                       cheapest candidate whose quality ≥ bar  ──▶ decision
                       (difficulty near threshold ⇒ flagged `uncertain`)
```

- **Useful untrained.** `DifficultyModel::default()` ships hand-set prior
  weights, so routing is sensible with zero data.
- **Learned from outcomes.** `DifficultyModel::train()` fits the predictor to
  logged preference data (label = 1 ⇒ the strong model was needed) by logistic
  regression — RouteLLM's training signal.
- **Self-improvable + measurable.** All tunables live in a serializable
  `RouterParams`, and `CostAwareRouter::evaluate()` is a deterministic offline
  score (strong-fraction / avg-cost / accuracy, LLMRouterBench-style). A
  `soul-rsi` loop can treat a parameter set as a Variant and keep only
  empirically-better routers.
- **Cost-aware across local fleets.** Effective cost folds latency into the
  dollar cost, so routing stays meaningful even when every model is a free
  local one.

## CLI — `avid-route`

Built by default (the `cli` feature). Install / run from the workspace:

```bash
cargo run -p avid-model-router --bin avid-route -- <SUBCOMMAND> ...
```

### route — pick a model and explain why

```bash
avid-route route "summarise this thread" --cap summarization
avid-route route "prove why the cluster deadlocks step by step" --cap analysis --json
avid-route route "write a small helper" --cap code-generation --cost-aversion 0.9
```

### explain — show the difficulty features

```bash
avid-route explain "design a partition-tolerant plan" --cap planning --cap analysis
```

### train / eval / calibrate

Data is JSON Lines, one record per line:

```json
{"query": "summarise the incident", "capabilities": ["summarization"], "label": 0.0}
{"query": "prove the root cause and design a fix", "capabilities": ["analysis"], "label": 1.0}
```

`label = 1.0` means a strong model was needed. Omit `label` for `route`/`explain`.

```bash
# Fit the difficulty model on logged outcomes, write tuned params.
avid-route train --data outcomes.jsonl --out params.json --epochs 500 --lr 0.3

# Score the router on a held-out set.
avid-route eval --data heldout.jsonl --params params.json

# Set the deferral operating point: ~30% of queries go to a strong model.
avid-route calibrate --data queries.jsonl --target-fraction 0.30 --out params.json
```

`--params <FILE>` loads tuned `RouterParams`; `--models <FILE>` loads a custom
model fleet (same JSON schema as the embedded `models.json`). Both default to
the built-in priors / fleet.

Capabilities accept kebab-case, snake_case, or short aliases: `code-generation`
(`codegen`, `code`), `code-review` (`review`), `planning` (`plan`), `analysis`
(`analyze`), `summarization` (`summary`), `creative`, `fast-chat` (`chat`).

## Library

```rust
use avid_model_router::{CostAwareRouter, Capability, RouterParams};

let router = CostAwareRouter::with_defaults();
let decision = router
    .route("analyze why this deadlocks and design a fix", &[Capability::Analysis])
    .expect("a model satisfies the capabilities");
println!("{} — {}", decision.profile.name, decision.reason);
```

Training and offline evaluation:

```rust
use avid_model_router::{DifficultyModel, QueryFeatures, Capability};

let samples = vec![
    (QueryFeatures::extract("hi", &[Capability::FastChat]), 0.0),
    (QueryFeatures::extract("prove the root cause step by step", &[Capability::Analysis]), 1.0),
];
let mut model = DifficultyModel::default();
let loss = model.train(&samples, 400, 0.3); // lower is better
```

## Roadmap

- Wire `CostAwareRouter` into `soullink-gateway`'s provider selection and train
  it on logged gateway outcomes.
- Add an LLMRouterBench-style dataset loader for offline benchmarking.
- Expose the router params as a `soul-rsi` Variant so the loop evolves routing
  policy under the empirical gate.
