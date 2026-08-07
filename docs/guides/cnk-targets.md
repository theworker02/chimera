# Guide: Chimera Nano-Kernel targets

## Host (Windows)
```bash
cargo test -p chimera-nano-kernel
cargo run -p chimera-nano-kernel --example host_boot
```

## Bare-metal check (no_std core only)
```bash
rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf x86_64-unknown-uefi
cargo check -p chimera-nano-kernel --no-default-features --target thumbv7em-none-eabihf
cargo check -p chimera-nano-kernel --no-default-features --features cortex-m --target thumbv7em-none-eabihf
```

## Scaffolding honesty
| Target | Status |
|---|---|
| Host std shim | **Runnable** |
| thumbv7em / riscv32 check | **Compile-verified** core |
| UEFI x86_64 | Stub + target available; **boot untested** |
| QUIC over smoltcp | **Not implemented** — use UDP frames on MCU, Quinn on host |

Linker scripts / PACs for real boards are out of scope for this phase.
