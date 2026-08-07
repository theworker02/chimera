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
        # crates.io rate-limits *new* crate publishes (a short burst, then roughly
        # one per 10 minutes). A 429 is expected for a large first release, so wait
        # out the server-supplied deadline and retry rather than aborting a release
        # that is already partly done.
        $attempt = 0
        while ($true) {
            $attempt++
            $oldEap = $ErrorActionPreference
            $ErrorActionPreference = "Continue"
            $out = cargo publish -p $crate --allow-dirty 2>&1 | Out-String
            $code = $LASTEXITCODE
            $ErrorActionPreference = $oldEap
            Write-Host ($out.Split("`n") | Select-Object -Last 8 | Out-String)

            if ($code -eq 0) { break }

            # Already published by an earlier interrupted run: treat as done so the
            # script is safe to re-run after a rate limit or a network failure.
            if ($out -match 'already exists on crates\.io|already (been )?uploaded') {
                Write-Host "    already published: $crate@$Version - skipping"
                break
            }

            if ($out -match '(?s)429 Too Many Requests.*?try again after ([^\r\n]+?)\s+and see') {
                $whenRaw = $Matches[1].Trim()
                try { $when = [datetimeoffset]::Parse($whenRaw).ToLocalTime() }
                catch { $when = (Get-Date).AddMinutes(11) }
                # Small buffer past the deadline to avoid a second 429.
                $wait = [int](($when - (Get-Date)).TotalSeconds) + 20
                if ($wait -lt 5) { $wait = 5 }
                Write-Host "    rate limited; waiting $wait s (until ~$when) then retrying $crate (attempt $attempt)"
                Start-Sleep -Seconds $wait
                continue
            }

            throw "publish failed for $crate - HALTING (partial release possible on crates.io)"
        }
        Wait-ForIndex $crate $Version
    }
}

Write-Host "OK: all crates $Mode completed"
