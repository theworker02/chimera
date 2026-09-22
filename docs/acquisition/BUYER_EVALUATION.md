# Buyer evaluation â€” Chimera

## Goal

In 15â€“45 minutes, verify the Product builds or runs as documented and that proprietary notices are present.

## Steps

1. Confirm root `LICENSE` is proprietary and `ACQUISITION.md` exists.
2. Skim `README.md` install/run claims.
3. Execute:

```
```mermaid
flowchart LR
  subgraph Mesh["P2P Mesh"]
    A[Node A<br/>Agent + Scheduler]
    B[Node B<br/>Agent + Scheduler]
    C[Node C<br/>Agent + Scheduler]
  end
  A <-- QUIC/TCP postcard --> B
  B <-- gossip multicast --> C
  A <-- steal / migrate --> C
  subgraph Fabric["Data + Memory"]
    FS[ChimeraFS CAS/DHT]
    MEM[ChimeraMEM DSM]
  end
  A --> FS
  B --> MEM
  C --> FS
  Intent[Declarative Intent] --> A
```
```mermaid
flowchart TB
  Intent["Intent / Agent"] -->|plan| Sched["Scheduler<br/>work-steal"]
  Intent -->|plan| Wasm["Wasmtime<br/>sandbox"]
  Intent -->|plan| Mem["ChimeraMEM<br/>soft DSM"]
  Sched <-->|"steal"| Wasm
  Wasm -->|"migrate"| Mem
  Sched -->|"prefetch"| FS["ChimeraFS<br/>BLAKE3 CAS / gossip DHT / VirtualMount / FUSE*"]
  Wasm -->|"I/O"| FS
  Mem -->|"pages"| FS
```
```bash
# From crates.io (after a real release)
cargo install chimeractl
cargo install chimera-boot
cargo install chimera-mesh --bin chimera

# Or fetch GitHub Release binaries (matches release.yml artifact names)
cargo binstall chimeractl
cargo binstall chimera-boot

```

4. Run tests if present (`npm test`, `pytest`, `cargo test`, `go test ./...`, etc.).
5. Record README vs observed behavior gaps in workpapers.

## Pass criteria

- [ ] Clone succeeds
- [ ] Documented happy path works **or** failure is explained
- [ ] Minimal path needs no surprise secrets
- [ ] License notices intact

*Updated: 2026-09-22*
