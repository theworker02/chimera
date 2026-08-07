# Chimera Nano-Kernel (CNK)

Silicon-agnostic `no_std` + `alloc` execution matrix for Chimera Phase 6.

## Features
| Feature | Default | Purpose |
|---|---|---|
| `host` | ✓ | std shim for Windows/desktop tests |
| `wasm-tier` | ✓ | wasmi interpreter |
| `pq` | ✓ | ML-KEM + ML-DSA handshake |
| `net` | ✓ | smoltcp framing + sim device |
| `uefi` / `cortex-m` / `riscv` | | bare-metal stubs |

## Host boot demo
```bash
cargo run -p chimera-nano-kernel --example host_boot
```

## Embedded check
```bash
cargo check -p chimera-nano-kernel --no-default-features --target thumbv7em-none-eabihf
```

## Honesty
Real UEFI / Cortex-M / RISC-V boots are **scaffolding**. Production-grade pieces on host: deterministic replay, block pool, softfloat/fixed-point, PQ handshake tests, smoltcp sim loopback.
