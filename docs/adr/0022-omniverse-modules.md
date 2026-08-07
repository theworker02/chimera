# ADR-0022: Omniverse modularization (28 crates)

## Status
Accepted (Phase 12)

## Context
Phases 1–11 delivered a working mesh in a large umbrella crate. Phase 12 splits capabilities into **exactly 28** independently usable modules so consumers can depend on one surface without pulling the full node.

## Layout
- Rust libraries/binaries: `crates/<module>/`
- Non-Rust packages: `packages/<module>/`
- Umbrella composer: root `chimera` package

## Layering (acyclic)
1. **Foundation:** `core-nano` → `crypto-quantum` (facade) → `transport-quic` (wire + QUIC)
2. **Storage/net:** `storage-cas`, `dht-routing`, `fuser-mount`, `network-bridge`, `consensus-dag`
3. **Execution:** `wasm-runtime`, `memory-fabric`, `compiler-jit`, `scheduler-rt`
4. **Autonomy:** `agent-swarm`, `telemetry-otel`, `inference-engine`
5. **Security/policy:** `rbac-auth`, `audit-ledger`, `compliance-tee`, `policy-engine`
6. **Edges:** `usb-daemon`, `cli-tool`, SDKs, gitops, ui/dashboard/audio
7. **Umbrella:** `chimera` may depend on all layers; crates must not depend upward on `chimera` except `usb-daemon` (portable binary)

## Rules
- No cycles between crates.
- Shared wire types live in `transport-quic` (not a 29th protocol crate).
- Prefer re-exports from the umbrella for backward-compatible `chimera::*` paths.

## Honesty
Some umbrella modules (`gateway`, `mgmt`, `fs` facade, `freight`, …) still compose multiple crates inside `chimera` itself. The 28 packages are the stable boundaries; further extraction can continue without breaking those APIs.
