# ADR-0006: Chimera Nano-Kernel (CNK)

## Status
Accepted (Phase 6)

## Context
Chimera must eventually run beyond desktop OS hosts — UEFI appliances, microcontrollers, and other silicon — while sharing framing, determinism, and security with the mesh.

## Decision
Introduce `chimera-nano-kernel` (`cnk/`): a `#![no_std]` + `alloc` core with:
- block-pool allocator, postcard framing, deterministic TxLog, softfloat/fixed-point
- wasmi interpreter tier (feature) vs host Wasmtime JIT
- smoltcp sim framing (feature) — **not** QUIC-over-smoltcp
- ML-KEM/ML-DSA hybrid handshake (feature)
- host shim for Windows tests; UEFI/Cortex-M/RISC-V **stubs only**

## Consequences
- Workspace stays green on Windows with default `host` features.
- Bare-metal targets verified via `cargo check --no-default-features --target …`.
- Real firmware boots remain future work (documented honestly).
