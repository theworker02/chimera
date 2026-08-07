#!/usr/bin/env bash
# Publish Chimera crates to crates.io in topological order.
# Usage:
#   ./scripts/publish-crates.sh --dry-run     # verify packaging (default)
#   ./scripts/publish-crates.sh --execute     # REAL publish (IRREVERSIBLE) + index waits
#
# First-release note: `cargo publish --dry-run` resolves versioned path-deps against
# crates.io. Until leaf crates exist on the index, dependents cannot complete a
# full dry-run verify. In --dry-run mode we therefore:
#   1) full `cargo publish --dry-run` for crates whose deps are already on crates.io
#      (or have no workspace path deps)
#   2) `cargo check -p` for the rest (path resolution), and still halt on errors
# After the first real leaf publish, subsequent dry-runs of dependents succeed.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE="dry-run"
for arg in "$@"; do
  case "$arg" in
    --dry-run) MODE="dry-run" ;;
    --execute) MODE="execute" ;;
    -h|--help)
      echo "Usage: $0 [--dry-run|--execute]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

# Topological publish order (leaves → dependents). Keep in sync with ADR-0024.
CRATES=(
  chimera-nano-kernel
  chimera-transport-quic
  chimera-rbac-auth
  chimera-compiler-jit
  chimera-compliance-tee
  chimera-inference-engine
  chimera-policy-engine
  chimera-consensus-dag
  chimera-audit-ledger
  chimera-storage-cas
  chimera-network-bridge
  chimera-crypto-quantum
  chimera-nexus
  chimera-dht-routing
  chimera-agent-swarm
  chimera-memory-fabric
  chimera-telemetry-otel
  chimera-wasm-runtime
  chimera-fuser-mount
  chimera-boot
  chimera-mesh
  chimera-usb-daemon
  chimeractl
)

VERSION="$(awk '
  /^\[workspace.package\]/ { in_wp=1; next }
  /^\[/ { in_wp=0 }
  in_wp && /^version/ {
    gsub(/"/, "", $3); print $3; exit
  }
' Cargo.toml)"
echo "Publishing Chimera workspace version=${VERSION} mode=${MODE}"

echo "==> cargo test --workspace (CI-safe features)"
cargo test --workspace --no-default-features --features "cnk,mgmt,nexus"

wait_for_index() {
  local name="$1"
  local ver="$2"
  local url="https://crates.io/api/v1/crates/${name}/${ver}"
  echo "    waiting for crates.io index: ${name}@${ver}"
  local i=0
  while (( i < 90 )); do
    if curl -fsSL -A "chimera-publish-script" "$url" >/dev/null 2>&1; then
      echo "    index ready: ${name}@${ver}"
      return 0
    fi
    i=$((i + 1))
    sleep 5
  done
  echo "ERROR: timed out waiting for ${name}@${ver} on crates.io" >&2
  return 1
}

crate_on_index() {
  local name="$1"
  local ver="$2"
  curl -fsSL -A "chimera-publish-script" "https://crates.io/api/v1/crates/${name}/${ver}" >/dev/null 2>&1
}

FAILED=0
for crate in "${CRATES[@]}"; do
  echo "==> ${MODE}: ${crate}"
  if [[ "$MODE" == "dry-run" ]]; then
    set +e
    out="$(cargo publish -p "$crate" --dry-run --allow-dirty 2>&1)"
    status=$?
    set -e
    echo "$out" | tail -n 20
    if [[ $status -eq 0 ]]; then
      continue
    fi
    if echo "$out" | grep -q 'no matching package named'; then
      echo "    note: dependency not yet on crates.io (first-release). Falling back to cargo check -p ${crate}"
      if ! cargo check -p "$crate"; then
        echo "ERROR: check failed for ${crate}" >&2
        FAILED=1
        break
      fi
      continue
    fi
    echo "ERROR: dry-run failed for ${crate}" >&2
    FAILED=1
    break
  else
    if ! cargo publish -p "$crate" --allow-dirty; then
      echo "ERROR: publish failed for ${crate} — HALTING (partial release possible)" >&2
      echo "Already attempted crates before this failure may be live on crates.io." >&2
      FAILED=1
      break
    fi
    wait_for_index "$crate" "$VERSION" || { FAILED=1; break; }
  fi
done

if (( FAILED != 0 )); then
  exit 1
fi
echo "OK: all crates ${MODE} completed"
