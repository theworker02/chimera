# ADR-0024: Global distribution & ecosystem publishing

## Status
Accepted (Phase 15)

## Context
Chimera ships many crates and two primary CLI binaries (`chimeractl`, `chimera-boot`). Several desirable crates.io names (`chimera`, `wasm-runtime`, `network-bridge`, `agent-swarm`, `policy-engine`, `chimera-core`) are **already taken**. Publishing is irreversible for claimed names.

## Decision — Package naming
| Role | crates.io name | Binary (if any) |
|---|---|---|
| Umbrella mesh node | `chimera-mesh` | `chimera` |
| CLI | `chimeractl` | `chimeractl` |
| USB flasher | `chimera-boot` | `chimera-boot` |
| All other libs | `chimera-<module>` | — |

The library crate name for the umbrella remains `chimera` (`[lib] name = "chimera"`) so dependents can `use chimera::…` while depending on package `chimera-mesh`.

Internal Cargo dependency **keys** keep short names via `package = "chimera-…"` renames to limit Rust `use` churn.

## Decision — Publish order & index propagation
Publish leaves before dependents (see `scripts/publish-crates.sh`). After each real publish, **poll** `https://crates.io/api/v1/crates/{name}/{version}` until HTTP success before continuing. A fixed sleep is insufficient; a failed mid-sequence halt makes partial releases obvious.

**First-release chicken-and-egg:** `cargo publish --dry-run` rewrites path+version deps to version-only and resolves them from crates.io. Until leaf crates exist on the index, dependents cannot complete a full dry-run verify. The dry-run script therefore falls back to `cargo check -p` for those crates (and still fails loud on real errors). After the first leaf publish, subsequent dry-runs of dependents succeed normally.

## Decision — CI features (`--all-features` forbidden)
`--all-features` enables platform-gated features (`fuse`, `userfaultfd`, `bridge-serial`, `bridge-bluetooth`, TEE backends) that cannot build on a generic runner. CI uses `CI_FEATURES=cnk,mgmt,nexus`.

## Decision — `dist` vs `release` profiles
- `release`: `opt-level = 3` for mesh throughput.
- `dist`: inherits release but `opt-level = "z"`, `strip = true`, `panic = "abort"` for shipped CLI size. Does not affect `cargo test` (uses `dev` / `test` profiles).

## Decision — Checksums
Release artifacts publish both SHA-256 (Homebrew / binstall ecosystem) and BLAKE3 (Chimera CAS alignment).

## Consequences
- Real `cargo publish` / `npm publish` / etc. are **not** run in development.
- Acceptance gate: every publishable crate passes `cargo publish --dry-run`.
- Homebrew formula lives as a template; the tap is a separate `homebrew-chimera` repo.
