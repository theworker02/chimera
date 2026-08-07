# Guide: Custom Wasm guests

## ABI
Export from your module:

| Export | Signature | Role |
|---|---|---|
| `memory` | linear memory | required |
| `chimera_alloc` | `(i32) -> i32` | allocate `len` bytes |
| `chimera_dealloc` | `(i32, i32)` | free |
| `chimera_execute` | `(in_ptr, in_len, out_ptr, out_cap) -> i32` | run slice; return bytes written or negative error |

### Input layout
`[u64 seed][u32 count][u32 pad][f32 × count]`

### Output layout
`[u64 checksum][u32 count][u32 pad][f32 × count]`

## Build the sample guest

```bash
cargo build -p chimera-guest --release --target wasm32-unknown-unknown
```

Artifact: `target/wasm32-unknown-unknown/release/chimera_guest.wasm`

## Run with custom module

```bash
cargo run -- --wasm path/to/module.wasm --demo-slices 2 --no-tui
```

If `--wasm` is omitted, Chimera loads an embedded WAT demo kernel.
