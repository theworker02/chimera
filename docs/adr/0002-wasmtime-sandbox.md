# ADR-0002: Wasmtime sandbox for untrusted compute

## Status
Accepted

## Context
Peers execute untrusted job slices. Native plugins are unsafe across trust boundaries and ABIs.

## Decision
Compile payloads to **WebAssembly** and execute in **Wasmtime** with fuel metering and store memory limits. Guest ABI: `chimera_alloc` / `chimera_dealloc` / `chimera_execute`.

## Consequences
- Cross-platform binaries (Windows/Linux/macOS).
- Live migration checkpoints linear memory + fuel; **call-stack IP is not fully portable** — resume re-enters guest with `checkpoint_offset` (documented limitation).
- Demo guest lives in `examples/guest`; embedded WAT fallback ships for zero-setup demos.
