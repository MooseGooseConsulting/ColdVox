#!/usr/bin/env pwsh

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ComposeFile = Join-Path $RepoRoot 'ops/parakeet/docker-compose.yml'
$ConfigPath = Join-Path $RepoRoot 'config/windows-parakeet.toml'
$HealthUrl = 'http://localhost:5092/health'

function Get-ParakeetHealthTimeoutSeconds {
    $value = $env:COLDVOX_PARAKEET_HEALTH_TIMEOUT_SECONDS
    $parsed = 0

    if (-not [string]::IsNullOrWhiteSpace($value) -and [int]::TryParse($value, [ref]$parsed) -and $parsed -gt 0) {
        return $parsed
    }

    180
}

$DefaultParakeetHealthTimeoutSeconds = Get-ParakeetHealthTimeoutSeconds

function Wait-ParakeetHealth {
    param(
        [int]$TimeoutSeconds = $DefaultParakeetHealthTimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $response = Invoke-RestMethod -Uri $HealthUrl -Method Get -TimeoutSec 5
            if ($response.status -eq 'ok') {
                return
            }
        } catch {
            Start-Sleep -Seconds 2
            continue
        }

        Start-Sleep -Seconds 2
    }

    throw "Parakeet HTTP container did not become healthy at $HealthUrl within $TimeoutSeconds seconds."
}

# Ensure the Python DLL from the UV-managed environment is in the PATH.
# This fixes the STATUS_DLL_NOT_FOUND (0xc0000135) crash.
Push-Location $RepoRoot
try {
    Write-Host "==> Ensuring canonical Parakeet CPU container is running..." -ForegroundColor Blue
    docker compose -f $ComposeFile up -d parakeet-cpu | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose up failed (exit code $LASTEXITCODE)."
    }
    Wait-ParakeetHealth

    if (-not (Test-Path $ConfigPath)) {
        throw "Missing Windows HTTP remote override: $ConfigPath"
    }
    $env:COLDVOX_CONFIG_PATH = $ConfigPath

    Write-Host "==> Detecting Python environment..." -ForegroundColor Blue
    $base = uv run python -c "import sys; print(sys.base_prefix)"

    if (-not $base) {
        throw "Could not find UV-managed Python. Run 'uv sync' first."
    }

    Write-Host "==> Adding $base to PATH..." -ForegroundColor Blue
    $env:PATH = "$base;$env:PATH"

    Write-Host "==> Starting ColdVox with canonical HTTP remote profile (Parakeet on cluster endpoint http://192.168.30.207:5092; local compose on localhost:5092 is the offline fallback)..." -ForegroundColor Green
    cargo run -p coldvox-app --bin coldvox --features http-remote,text-injection
} finally {
    Pop-Location
}
