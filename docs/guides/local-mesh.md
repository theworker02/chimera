# Guide: Local mesh setup

## Prerequisites
- Rust stable (`rustup`)
- Windows, macOS, or Linux on the same L2/LAN (UDP multicast)

## Bootstrap two nodes

```bash
# Terminal A
cargo run -- --name alpha --tcp-bind 0.0.0.0:7400 --quic-bind 0.0.0.0:7401 --demo-slices 4 --no-tui

# Terminal B (different ports)
cargo run -- --name beta --tcp-bind 0.0.0.0:7402 --quic-bind 0.0.0.0:7403 --no-tui
```

With TUI (default): omit `--no-tui`, press `Tab` to switch Topology / ChimeraFS / ChimeraMEM / Agents, `q` to quit.

## Intent-driven job

```bash
cargo run -- --no-tui --intent "name=preview latency<200ms privacy=local render=hd slices=6"
```

## Data directory
Default `./data` holds pipeline chunks, ChimeraFS CAS blocks, and checkpoints.
