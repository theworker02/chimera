# Poll crates.io until all publishable crates are LIVE at the workspace version,
# then regenerate README markers and commit+push to origin main.
# Does NOT start or kill any publish process.
#
# Usage:
#   .\scripts\watch-crates-readme-publish.ps1
#   .\scripts\watch-crates-readme-publish.ps1 -PollSeconds 90 -Push
param(
    [int]$PollSeconds = 90,
    [bool]$Push = $true
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
$env:Path = "C:\Users\matth\.cargo\bin;" + $env:Path

$Sync = Join-Path $PSScriptRoot "sync-crates-readme.ps1"
$Ua = "chimera-watch-crates-readme (github.com/theworker02/chimera)"
$started = Get-Date
$round = 0

Write-Host "Watcher started at $started (poll every ${PollSeconds}s). Will not touch publish processes."

while ($true) {
    $round++
    Write-Host ""
    Write-Host "==== poll #$round $(Get-Date -Format o) ===="
    & $Sync -CheckOnly
    $code = $LASTEXITCODE
    if ($code -eq 0) {
        Write-Host "All crates LIVE. Updating README..."
        & $Sync -RequireAll
        if ($LASTEXITCODE -ne 0) { throw "sync-crates-readme.ps1 failed" }

        # git writes CRLF/LF notices to stderr; with $ErrorActionPreference=Stop that
        # becomes a terminating NativeCommandError even when exit code is 0.
        $oldEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        git add README.md 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $ErrorActionPreference = $oldEap; throw "git add failed" }

        $status = git status --porcelain README.md
        if (-not $status) {
            Write-Host "README already committed with full crates list."
        } else {
            git commit -m "Document all crates.io package links after v0.1.0 publish" 2>&1 | Out-Host
            if ($LASTEXITCODE -ne 0) { $ErrorActionPreference = $oldEap; throw "git commit failed" }
        }

        if ($Push) {
            git push origin HEAD:main 2>&1 | Out-Host
            if ($LASTEXITCODE -ne 0) { $ErrorActionPreference = $oldEap; throw "git push failed" }
        }
        $ErrorActionPreference = $oldEap

        $sha = git rev-parse HEAD
        Write-Host "DONE sha=$sha"
        Write-Host "LIVE=23/23"
        exit 0
    }

    Write-Host "Not complete yet; sleeping ${PollSeconds}s (be polite to crates.io)..."
    Start-Sleep -Seconds $PollSeconds
}
