#Requires -Version 7
<#
.SYNOPSIS
    Brings up the parakeet-cpu HTTP container and runs the live integration tests.

.DESCRIPTION
    Invoked by `just integration-parakeet`. Uses ops/parakeet/docker-compose.yml to
    start the parakeet-cpu service, waits for GET /health to return status=ok, then
    runs both the plugin-level and app-level live integration tests with --ignored.

.NOTES
    Health timeout is configurable via COLDVOX_PARAKEET_HEALTH_TIMEOUT_SECONDS (default 180).
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ComposeFile = Join-Path $RepoRoot 'ops/parakeet/docker-compose.yml'
if (-not (Test-Path $ComposeFile)) {
    throw "docker-compose file not found: $ComposeFile"
}

$HealthTimeoutSeconds = if ($env:COLDVOX_PARAKEET_HEALTH_TIMEOUT_SECONDS) {
    [int]$env:COLDVOX_PARAKEET_HEALTH_TIMEOUT_SECONDS
} else { 180 }

Write-Host "==> docker compose up -d parakeet-cpu"
# NOTE: the container is left running on exit so subsequent cargo test invocations
# hit a warm fixture. Tear down manually via `docker compose -f <file> down` when done.
docker compose -f $ComposeFile up -d parakeet-cpu
if ($LASTEXITCODE -ne 0) { throw "docker compose up failed (exit $LASTEXITCODE)" }

Write-Host "==> waiting for http://localhost:5092/health (timeout ${HealthTimeoutSeconds}s)"
$deadline = (Get-Date).AddSeconds($HealthTimeoutSeconds)
$healthy = $false
$lastHealthError = $null
while ((Get-Date) -lt $deadline) {
    try {
        $r = Invoke-WebRequest -Uri 'http://localhost:5092/health' -TimeoutSec 5 -UseBasicParsing -ErrorAction Stop
        if ($r.StatusCode -eq 200 -and $r.Content -match '"status"\s*:\s*"ok"') {
            $healthy = $true
            break
        }
    } catch {
        $lastHealthError = $_
        Write-Verbose "Health probe failed: $($_.Exception.Message)"
    }
    Start-Sleep -Seconds 2
}
if (-not $healthy) {
    if ($lastHealthError) {
        throw "parakeet-cpu did not report healthy at /health within ${HealthTimeoutSeconds}s. Last probe error: $($lastHealthError.Exception.Message)"
    }
    throw "parakeet-cpu did not report healthy at /health within ${HealthTimeoutSeconds}s"
}
Write-Host "    OK"

Push-Location $RepoRoot
try {
    Write-Host "==> cargo test -p coldvox-stt --test http_remote_live -- --ignored"
    cargo test -p coldvox-stt --features http-remote --test http_remote_live --locked -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw "http_remote_live failed (exit $LASTEXITCODE)" }

    Write-Host "==> cargo test -p coldvox-app --test http_remote_wiring_live -- --ignored"
    cargo test -p coldvox-app --features http-remote --test http_remote_wiring_live --locked -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw "http_remote_wiring_live failed (exit $LASTEXITCODE)" }

    Write-Host "==> integration-parakeet: OK"
} finally {
    Pop-Location
}
