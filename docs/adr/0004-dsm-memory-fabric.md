# ADR-0004: DSM memory fabric (ChimeraMEM)

## Status
Accepted

## Context
Shared working sets and live Wasm migration need a unified address space without kernel RDMA.

## Decision
Implement a **portable soft page-table DSM** over QUIC. Linux may enable `userfaultfd` behind `--features userfaultfd`. Consistency knobs: CRDT regions (vector clocks, G-Counter, OR-Set) vs ownership leases for linearizable pages. Tiering: HotRam → PeerCache → ColdFs (+ GPU hints).

## Consequences
- Works on Windows for demos.
- Page faults fetch over bulk streams; control plane stays prioritized.
- Migration packetizes linear memory (XOR deltas available for similar pages).
