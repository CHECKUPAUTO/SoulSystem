# Bus Specification — SoulSystem Message Bus

## Overview

The message bus is the central communication channel between SoulSystem modules.
It uses `tokio::sync::broadcast` for publish/subscribe semantics — any module
can publish messages and any module can subscribe to receive them.

## Architecture

```
Publisher A ──┐
Publisher B ──┤
Publisher C ──┼──> Bus (broadcast channel, capacity 256) ──┬──> Subscriber 1
               │                                           ├──> Subscriber 2
               │                                           └──> Subscriber 3
               │
               └──> Missed messages are dropped ("lagged" notification)
```

## Message Types

### `Message::HnnStatus`
Published by: HNN monitor (external)
Consumed by: AnomalyWatcher, Dashboard

```rust
HnnStatus {
    ticks_per_sec: u64,  // Current HNN tick rate
}
```

### `Message::SynergyDetection`
Published by: Synergy engine (external)
Consumed by: Dashboard

```rust
SynergyDetection {
    module: String,      // Module that detected synergy
    description: String, // Description of the synergy found
}
```

### `Message::AvidDiscovery`
Published by: AVID service
Consumed by: Dashboard, SoulMemory (via Clawd)

```rust
AvidDiscovery {
    source: String,  // Source (arXiv, web, etc.)
    summary: String, // Discovery summary
}
```

### `Message::EvolveOptimization`
Published by: OpenEvolve/GEPA
Consumed by: Dashboard

```rust
EvolveOptimization {
    crate_name: String, // Crate that was optimized
    score: f64,         // Fitness score after optimization
}
```

## Subscription Rules

### Who publishes what
| Publisher | Message | Frequency |
|-----------|---------|-----------|
| SoulLink HNN mesh | `HnnStatus` | ~1/sec per organ |
| AVID orchestrator | `AvidDiscovery` | On discovery |
| GEPA evolution engine | `EvolveOptimization` | On optimization |
| Synergy engine | `SynergyDetection` | On detection |

### Who listens to what
| Subscriber | Messages | Purpose |
|------------|----------|---------|
| `AnomalyWatcher` | `HnnStatus` | Detect tick rate drops >40% |
| `DevDashboard` | All | Display in web UI |
| `SoulMemory` (via Clawd) | `AvidDiscovery` | Store research discoveries |
| `Telemetry` | All (via spans) | Export to OTLP |

## Usage Examples

### Publishing
```rust
use soulsystem::bus::{Bus, Message};

let bus = Bus::new(256);
bus.publish(Message::HnnStatus { ticks_per_sec: 42_000 });
bus.publish(Message::AvidDiscovery {
    source: "arxiv".into(),
    summary: "New quantum computing paper".into(),
});
```

### Subscribing
```rust
use soulsystem::bus::Bus;
use std::sync::Arc;
use tokio_stream::StreamExt;

async fn listen(bus: Arc<Bus>) {
    let mut rx = bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(Message::HnnStatus { ticks_per_sec }) => {
                println!("HNN: {} ticks/sec", ticks_per_sec);
            }
            Ok(msg) => {
                println!("Other message: {:?}", msg);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("Dropped {} messages", n);
            }
            Err(_) => break,
        }
    }
}
```

## Capacity & Backpressure

- Default capacity: 256 messages
- When full, oldest messages are dropped
- Subscribers that fall behind receive `RecvError::Lagged(n)` indicating how many messages were missed
- No acknowledgement or redelivery — the bus is fire-and-forget
- Critical data should be stored in SoulMemory or AuditLog, not relied upon via bus

## Error Handling

- `publish()` silently drops if no subscribers exist
- Subscribers must handle `Lagged` errors
- Channel closure happens when all senders are dropped — subscribers get `RecvError::Closed`
