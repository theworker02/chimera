# Chimera Nexus

Real-time game-engine / simulation interop for Chimera (Phase 8).

## Embed (C ABI)

```c
#include "chimera_nexus.h"
NexusNode *n = chimera_nexus_init(0); /* 60 FPS budget */
chimera_nexus_tick(n);
chimera_nexus_shutdown(n);
```

Header: [`include/chimera_nexus.h`](./include/chimera_nexus.h)  
Example: [`examples/example.c`](./examples/example.c) (syntax sample — compile when MSVC/clang available)

## WIT

[`wit/nexus.wit`](./wit/nexus.wit) — Component Model contract. wit-bindgen wiring is optional; see ADR-0012.

## Honesty
No Godot GDExtension / Unreal module was built or run here. Unit tests cover frame budget, ECS migration, and prediction rollback on the host.
