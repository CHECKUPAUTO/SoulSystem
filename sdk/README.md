# SoulSystem SDKs

SDKs multi-langages pour interagir avec le bus SoulSystem et AVID.

## Python

```python
# Installation (future: pip install soulsystem)
import soulsystem
bus = soulsystem.Bus()
bus.publish({"type": "test", "data": "hello"})
```

## TypeScript

```typescript
// Installation (future: npm install @soulsystem/sdk)
import { Bus } from '@soulsystem/sdk';
const bus = new Bus();
bus.publish({ type: 'test', data: 'hello' });
```

## Architecture

Les SDKs utilisent FFI (C ABI) via le crate `soulsystem-sdk` qui expose
des fonctions C pour se connecter au bus et appeler l'API AVID.
