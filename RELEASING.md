# Releasing Chimera

**Do not publish from a laptop unless you intend an irreversible crates.io release.**

## Prerequisites
- Tag will be `vX.Y.Z` matching `[workspace.package] version`
- `CRATES_IO_TOKEN` secret configured for the `crates-io` GitHub Environment
- GitHub Pages enabled for the official Pages deploy workflow
- All crates pass locally:

```bash
cargo test --workspace --no-default-features --features "cnk,mgmt,nexus"
./scripts/publish-crates.sh --dry-run
# Windows:
#   .\scripts\publish-crates.ps1 -DryRun
mdbook build docs/
```

## Tag → release sequence
1. Bump `version` in `[workspace.package]` (and verify path+version deps still match).
2. Run `./scripts/publish-crates.sh --dry-run` — must be green for **every** publishable crate.
3. Commit, tag `vX.Y.Z`, push the tag (this triggers `.github/workflows/release.yml`).
4. Workflow jobs:
   - validate + test (CI-safe features)
   - `publish-crates.sh --dry-run` again
   - on tag (and not dry-run dispatch): **real** crates.io publish with index polling
   - cross-compile `chimeractl` + `chimera-boot` with `--profile dist`
   - upload SHA256 + BLAKE3 checksums via `softprops/action-gh-release@v2`
   - build mdBook → GitHub Pages

## crates.io propagation caveat
Dependent crates cannot publish until the index serves each new version. The publish script **polls** the crates.io API (up to ~7.5 minutes per crate) and **halts on first failure**.

### If a partial publish occurs
1. Note which crate failed and which earlier crates already succeeded (script logs).
2. **Do not** change versions of already-published crates (yank only if the release is unsafe).
3. Fix the failing crate, wait for index health, then re-run publish starting at the failed crate (or bump patch and start a new tag — prefer completing the same version if nothing bad was published).
4. Yank with `cargo yank` only for critically broken versions; names remain claimed forever.

## Artifact names (must match binstall metadata)
| Binary | Linux amd64 | Linux arm64 | Windows | macOS |
|---|---|---|---|---|
| chimeractl | `chimeractl-linux-amd64` | `chimeractl-linux-arm64` | `chimeractl-windows-amd64.exe` | `chimeractl-macos-universal2` |
| chimera-boot | `chimera-boot-linux-amd64` | `chimera-boot-linux-arm64` | `chimera-boot-windows-amd64.exe` | `chimera-boot-macos-universal2` |

## Dry-run rehearsal without publishing
`workflow_dispatch` with `dry_run: true` exercises the pipeline without crates.io publish or release upload.
