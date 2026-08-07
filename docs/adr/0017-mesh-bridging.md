# ADR-0017: Mesh bridging & retro-hardware

## Status
Accepted (Phase 10)

## Context
Some peers cannot speak QUIC (MCUs, serial links, legacy LANs).

## Decision
- Introduce a `BridgeFrame` length-prefixed envelope compatible with the CNK framing story.
- **Working adapter:** plain TCP (`TcpBridgeEndpoint`) with in-process exchange tests.
- **Stubs:** `--features bridge-serial` and `bridge-bluetooth` expose adapters that return explicit roadmap errors (no hardware in CI).

## Honesty
Bluetooth/serial are not implemented. Planet-scale bridging is not claimed. Microcontroller peers can reuse CNK no_std frames once a serial adapter is filled in.

## Consequences
Document TCP as the supported legacy path; keep stubs feature-gated.
