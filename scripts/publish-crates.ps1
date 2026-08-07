# Publish Chimera crates to crates.io in topological order (Windows).
# Usage:
#   .\scripts\publish-crates.ps1 -DryRun
#   .\scripts\publish-crates.ps1 -Execute   # IRREVERSIBLE
param(
    [switch]$DryRun,
    [switch]$Execute
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

if (-not $DryRun -and -not $Execute) { $DryRun = $true }
if ($DryRun -and $Execute) { throw "Pass only one of -DryRun / -Execute" }

$Mode = if ($Execute) { "execute" } else { "dry-run" }

$Crates = @(
    "chimera-nano-kernel",
    "chimera-transport-quic",
    "chimera-rbac-auth",
    "chimera-compiler-jit",
    "chimera-compliance-tee",
    "chimera-inference-engine",
    "chimera-policy-engine",
    "chimera-consensus-dag",
    "chimera-audit-ledger",
    "chimera-storage-cas",
    "chimera-network-bridge",
    "chimera-crypto-quantum",
    "chimera-nexus",
    "chimera-dht-routing",
    "chimera-agent-swarm",
    "chimera-memory-fabric",
    "chimera-telemetry-otel",
    "chimera-wasm-runtime",
    "chimera-fuser-mount",
    "chimera-boot",
    "chimera-mesh",
    "chimera-usb-daemon",
    "chimeractl"
)

$Version = (
    Select-String -Path Cargo.toml -Pattern '^\s*version\s*=\s*"([^"]+)"' |
    Select-Object -First 1
).Matches[0].Groups[1].Value
Write-Host "Publishing Chimera workspace version=$Version mode=$Mode"

Write-Host "==> cargo test --workspace (CI-safe features)"
cargo test --workspace --no-default-features --features "cnk,mgmt,nexus"
if ($LASTEXITCODE -ne 0) { throw "tests failed" }

function Wait-ForIndex([string]$Name, [string]$Ver) {
    $url = "https://crates.io/api/v1/crates/$Name/$Ver"
    Write-Host "    waiting for crates.io index: $Name@$Ver"
    for ($i = 0; $i -lt 90; $i++) {
        try {
            Invoke-WebRequest -Uri $url -Headers @{ "User-Agent" = "chimera-publish-script" } -UseBasicParsing -TimeoutSec 15 | Out-Null
            Write-Host "    index ready: $Name@$Ver"
            return
        } catch {
            Start-Sleep -Seconds 5
        }
    }
    throw "timed out waiting for $Name@$Ver on crates.io"
}

foreach ($crate in $Crates) {
    Write-Host "==> ${Mode}: $crate"
    if ($DryRun) {
        $oldEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        $out = cargo publish -p $crate --dry-run --allow-dirty 2>&1 | Out-String
        $code = $LASTEXITCODE
        $ErrorActionPreference = $oldEap
        Write-Host ($out.Split("`n") | Select-Object -Last 15 | Out-String)
        if ($code -eq 0) { continue }
        if ($out -match 'no matching package named') {
            Write-Host "    note: dependency not yet on crates.io (first-release). Falling back to cargo check -p $crate"
            cargo check -p $crate
            if ($LASTEXITCODE -ne 0) { throw "check failed for $crate" }
            continue
        }
        throw "dry-run failed for $crate"
    } else {
        cargo publish -p $crate --allow-dirty
        if ($LASTEXITCODE -ne 0) {
            throw "publish failed for $crate - HALTING (partial release possible on crates.io)"
        }
        Wait-ForIndex $crate $Version
    }
}

Write-Host "OK: all crates $Mode completed"
