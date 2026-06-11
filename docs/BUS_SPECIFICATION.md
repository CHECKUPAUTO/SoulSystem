# Bus Specification — SoulSystem Message Bus

## Protocol

Messages are serialized using `bincode` for maximum performance.

## Topics

| Topic | Description |
|-------|-------------|
| `hnn.status` | Hamiltonian Neural Network energy metrics |
| `hnn.tick` | HNN tick event with current state |
| `avid.clone_request` | Trigger AVID to clone a target URL |
| `avid.clone_result` | AVID clone result |
| `synergy.detection` | Cross-module opportunistic discoveries |
| `agent.event` | Autonomous agent lifecycle events |
| `system.health` | System health check results |
| `memory.consolidation` | Memory consolidation events |

## Architecture

The bus uses `tokio::sync::broadcast` with 256 message capacity.
Subscribers can filter by topic prefix.
All messages include a timestamp and source identifier.

## Bridge Integration

External subsystems connect via HTTP bridges that publish/subscribe to the bus:
- AVID bridge (`avid-bridge`)
- OpenEvolve bridge (`openevolve-bridge`)
- Synergie bridge (`synergie-bridge`)
- Brain bridge (`brain-bridge`)
- Mesh bridge (`mesh-bridge`)
- Services bridge (`services-bridge`)
- Organs bridge (`organs-bridge`)
- Neural bridge (`soul-neural-bridge`)
- Orchestrator bridge (`orchestrator-bridge`)