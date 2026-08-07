# Contributing

Thanks for helping grow the Chimera mesh.

## Workflow
1. Fork & branch from `main`.
2. `cargo fmt && cargo clippy -- -D warnings` (when clippy is available).
3. `cargo test && cargo check`.
4. Open a PR with a clear summary + test plan.

## Design rules
- Prefer postcard + BLAKE3 over ad-hoc formats.
- Never starve control frames for bulk I/O.
- Keep Windows default builds free of FUSE / userfaultfd / ZK / ML deps (feature-gate them).
- Document architectural choices as ADRs under `docs/adr/`.

## Brand
Follow `brand/brand.md`. Palette: void `#0A0A0C`, cyan `#00F0FF`, amber `#FFB800`.

## Security
Do not commit secrets. Report vulnerabilities privately when possible. Mesh demos use self-signed QUIC certs — not production PKI.
