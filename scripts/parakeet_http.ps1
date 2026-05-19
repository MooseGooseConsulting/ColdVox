#Requires -Version 7
<#
.SYNOPSIS
    Manage the canonical local Parakeet HTTP container.

.DESCRIPTION
    Thin wrapper used by the justfile for the wave-1 `parakeet-*` lifecycle
    commands. The canonical CPU service is `parakeet-cpu` from
    ops/parakeet/docker-compose.yml and serves OpenAI-compatible STT on
    http://localhost:5092.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Up', 'Down', 'Health', 'Logs')]
    [string]$Action,

    [ValidateRange(1, 10000)]
    [int]$Tail = 200
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ComposeFile = Join-Path $RepoRoot 'ops/parakeet/docker-compose.yml'
$HealthUrl = 'http://localhost:5092/health'

function Assert-ComposeFile {
    if (-not (Test-Path $ComposeFile)) {
        throw "Missing Parakeet compose file: $ComposeFile"
    }
}

function Invoke-DockerCompose {
    param([string[]]$ArgumentList)

    Assert-ComposeFile
    & docker compose -f $ComposeFile @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose $($ArgumentList -join ' ') failed with exit code $LASTEXITCODE."
    }
}

switch ($Action) {
    'Up' {
        Invoke-DockerCompose -ArgumentList @('up', '-d', 'parakeet-cpu')
    }
    'Down' {
        Invoke-DockerCompose -ArgumentList @('down')
    }
    'Health' {
        $response = Invoke-RestMethod -Uri $HealthUrl -Method Get -TimeoutSec 10
        if ($response.status -ne 'ok') {
            $body = $response | ConvertTo-Json -Compress
            throw "Unexpected Parakeet health response from ${HealthUrl}: $body"
        }

        $response | ConvertTo-Json -Compress
    }
    'Logs' {
        Invoke-DockerCompose -ArgumentList @('logs', "--tail=$Tail", 'parakeet-cpu')
    }
}