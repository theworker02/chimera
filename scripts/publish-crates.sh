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
  chimera-sql
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
    if echo "$out" | grep -Eq 'no matching package named|failed to select a version for the requirement|candidate versions found which didn.t match'; then
      echo "    note: dependency not yet on crates.io at ${VERSION} (first-release). Falling back to cargo check -p ${crate}"
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
    # crates.io rate-limits *new* crate publishes (a short burst, then roughly one
    # per 10 minutes). For a large first release a 429 is expected, so wait out the
    # server-supplied deadline and retry instead of aborting a partly-done release.
    attempt=0
    while :; do
      attempt=$((attempt + 1))
      set +e
      out="$(cargo publish -p "$crate" --allow-dirty 2>&1)"
      status=$?
      set -e
      echo "$out" | tail -n 8

      if [[ $status -eq 0 ]]; then
        break
      fi

      # Already published by an earlier interrupted run: treat as done so the
      # script is safe to re-run after a rate limit or a network failure.
      if echo "$out" | grep -Eq 'already exists on crates\.io|already (been )?uploaded'; then
        echo "    already published: ${crate}@${VERSION} — skipping"
        break
      fi

      if echo "$out" | grep -q '429 Too Many Requests'; then
        when="$(echo "$out" | sed -n 's/.*try again after \(.*\) and see.*/\1/p' | head -n 1)"
        wait_s=""
        if [[ -n "$when" ]]; then
          target="$(date -u -d "$when" +%s 2>/dev/null || true)"
          if [[ -n "$target" ]]; then
            wait_s=$(( target - $(date -u +%s) + 20 ))
          fi
        fi
        # Fall back to the documented ~10 minute window if the deadline is unparseable.
        if [[ -z "$wait_s" || "$wait_s" -lt 5 ]]; then
          wait_s=660
        fi
        echo "    rate limited; waiting ${wait_s}s then retrying ${crate} (attempt ${attempt})"
        sleep "$wait_s"
        continue
      fi

      echo "ERROR: publish failed for ${crate} — HALTING (partial release possible)" >&2
      echo "Already attempted crates before this failure may be live on crates.io." >&2
      FAILED=1
      break 2
    done
    wait_for_index "$crate" "$VERSION" || { FAILED=1; break; }
  fi
done

if (( FAILED != 0 )); then
  exit 1
fi
echo "OK: all crates ${MODE} completed"
