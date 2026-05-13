#!/usr/bin/env pwsh

param(
    [ValidateSet('Preflight', 'Smoke', 'Live')]
    [string]$Mode = 'Live',

    [ValidateRange(1, 600)]
    [int]$RuntimeSeconds = 30,

    [ValidateRange(10, 2000)]
    [int]$TailLines = 200
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Timestamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$ArtifactRoot = Join-Path $RepoRoot "logs/windows-validation/$Timestamp-$($Mode.ToLowerInvariant())"
$ComposeFile = Join-Path $RepoRoot 'ops/parakeet/docker-compose.yml'
$ConfigPath = Join-Path $RepoRoot 'config/windows-parakeet.toml'
$LogPath = Join-Path $RepoRoot 'logs/coldvox.log'
$TestWavPath = Join-Path $RepoRoot 'crates/app/test_data/test_1.wav'
$HealthUrl = 'http://localhost:5092/health'
$TranscriptionsUrl = 'http://localhost:5092/v1/audio/transcriptions'
$FeatureList = 'http-remote,text-injection-enigo'

function Get-ParakeetHealthTimeoutSeconds {
    $value = $env:COLDVOX_PARAKEET_HEALTH_TIMEOUT_SECONDS
    $parsed = 0

    if (-not [string]::IsNullOrWhiteSpace($value) -and [int]::TryParse($value, [ref]$parsed) -and $parsed -gt 0) {
        return $parsed
    }

    180
}

$DefaultParakeetHealthTimeoutSeconds = Get-ParakeetHealthTimeoutSeconds
$TranscriptionMaxTimeSeconds = 180
$TranscriptionProcessTimeoutSeconds = 200

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Ok {
    param([string]$Message)
    Write-Host "OK: $Message" -ForegroundColor Green
}

function Assert-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found on PATH: $Name"
    }
}

function New-ArtifactDirectories {
    New-Item -ItemType Directory -Force -Path $ArtifactRoot | Out-Null
}

function Invoke-LoggedProcess {
    param(
        [string]$Name,
        [string]$FilePath,
        [string[]]$ArgumentList,
        [int]$TimeoutSeconds = 0
    )

    $stdout = Join-Path $ArtifactRoot "$Name.stdout.log"
    $stderr = Join-Path $ArtifactRoot "$Name.stderr.log"

    Write-Step $Name
    # Use ProcessStartInfo.ArgumentList (not Start-Process -ArgumentList) so arguments
    # containing spaces (e.g., $RepoRoot-derived paths) are quoted correctly.
    # See PowerShell/PowerShell#5576 for the Start-Process -ArgumentList quoting gap.
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $RepoRoot
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    if ($null -ne $ArgumentList) {
        foreach ($arg in $ArgumentList) {
            $startInfo.ArgumentList.Add([string]$arg)
        }
    }

    $process = [System.Diagnostics.Process]@{ StartInfo = $startInfo }
    [void]$process.Start()

    # Drain stdout/stderr asynchronously to avoid deadlock when either buffer fills.
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()

    $timedOut = $false
    if ($TimeoutSeconds -gt 0) {
        $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
        while (-not $process.HasExited -and (Get-Date) -lt $deadline) {
            Start-Sleep -Seconds 1
            $process.Refresh()
        }

        if (-not $process.HasExited) {
            $timedOut = $true
            & taskkill.exe /PID $process.Id /T /F | Out-Null
            $process.WaitForExit()
        }
    } else {
        $process.WaitForExit()
    }

    $stdoutContent = $stdoutTask.GetAwaiter().GetResult()
    $stderrContent = $stderrTask.GetAwaiter().GetResult()
    Set-Content -Path $stdout -Value $stdoutContent -NoNewline -Encoding UTF8
    Set-Content -Path $stderr -Value $stderrContent -NoNewline -Encoding UTF8

    foreach ($path in @($stdout, $stderr)) {
        if (Test-Path $path) {
            $tail = Get-Content $path | Select-Object -Last $TailLines
            if ($tail) {
                Write-Host ""
                Write-Host "$([System.IO.Path]::GetFileName($path)) tail:" -ForegroundColor DarkGray
                $tail | ForEach-Object { Write-Host $_ }
            }
        }
    }

    if ($process.ExitCode -ne 0 -and -not $timedOut) {
        throw "Command '$Name' failed with exit code $($process.ExitCode)."
    }

    [pscustomobject]@{
        ExitCode = $process.ExitCode
        TimedOut = $timedOut
        StdoutPath = $stdout
        StderrPath = $stderr
    }
}

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

function Copy-RuntimeLog {
    if (-not (Test-Path $LogPath)) {
        Write-Warning "Runtime log not found at $LogPath."
        Set-Content (Join-Path $ArtifactRoot 'coldvox.log') -Value '' -Encoding UTF8
        Set-Content (Join-Path $ArtifactRoot 'coldvox.log.tail') -Value '' -Encoding UTF8
        return
    }

    Copy-Item $LogPath (Join-Path $ArtifactRoot 'coldvox.log') -Force
    Get-Content $LogPath | Select-Object -Last 120 | Set-Content (Join-Path $ArtifactRoot 'coldvox.log.tail') -Encoding UTF8
}

function Invoke-Preflight {
    Write-Step 'preflight'
    New-ArtifactDirectories

    foreach ($command in @('cargo', 'curl.exe', 'docker', 'pwsh', 'taskkill.exe')) {
        Assert-Command $command
    }

    if (-not $IsWindows) {
        throw 'This validation wrapper only runs on Windows.'
    }

    if (-not (Test-Path $ComposeFile)) {
        throw "Missing compose file: $ComposeFile"
    }

    if (-not (Test-Path $ConfigPath)) {
        throw "Missing Windows HTTP remote override: $ConfigPath"
    }

    if (-not (Test-Path $TestWavPath)) {
        throw "Missing test WAV file: $TestWavPath"
    }

    Invoke-LoggedProcess -Name 'docker-ps' -FilePath 'docker' -ArgumentList @('ps') | Out-Null
    Invoke-LoggedProcess -Name 'parakeet-up' -FilePath 'docker' -ArgumentList @('compose', '-f', $ComposeFile, 'up', '-d', 'parakeet-cpu') | Out-Null

    Write-Step 'parakeet-health'
    Wait-ParakeetHealth
    Write-Ok 'Parakeet CPU container is healthy'

    $health = Invoke-LoggedProcess -Name 'parakeet-health-http' -FilePath 'curl.exe' -ArgumentList @('-sS', $HealthUrl)
    $healthBody = Get-Content $health.StdoutPath -Raw
    if ($healthBody -notmatch '"status"\s*:\s*"ok"') {
        throw "Unexpected health response: $healthBody"
    }

    $transcription = Invoke-LoggedProcess -Name 'parakeet-transcription-http' -FilePath 'curl.exe' -ArgumentList @(
        '-sS',
        '--fail-with-body',
        '--max-time', [string]$TranscriptionMaxTimeSeconds,
        '-X', 'POST',
        $TranscriptionsUrl,
        '-F', "file=@$TestWavPath",
        '-F', 'model=parakeet-tdt-0.6b-v2',
        '-F', 'response_format=json'
    ) -TimeoutSeconds $TranscriptionProcessTimeoutSeconds
    $transcriptionBody = Get-Content $transcription.StdoutPath -Raw
    if ($transcriptionBody -notmatch '"text"\s*:') {
        throw "Unexpected transcription response: $transcriptionBody"
    }

    [pscustomobject]@{
        HealthBody = $healthBody.Trim()
        TranscriptionBody = $transcriptionBody.Trim()
    }
}

function Set-RunEnvironment {
    $env:RUST_LOG = 'info'
    $env:COLDVOX_LOG_RETENTION_DAYS = '0'
    $env:COLDVOX_CONFIG_PATH = $ConfigPath
}

function Invoke-Smoke {
    $preflight = Invoke-Preflight
    Set-RunEnvironment

    $help = Invoke-LoggedProcess -Name 'coldvox-help' -FilePath 'cargo' -ArgumentList @(
        'run',
        '-p', 'coldvox-app',
        '--bin', 'coldvox',
        '--features', $FeatureList,
        '--quiet',
        '--locked',
        '--',
        '--help'
    )
    if ($help.ExitCode -ne 0) {
        throw "coldvox --help failed with exit code $($help.ExitCode)."
    }

    $devices = Invoke-LoggedProcess -Name 'coldvox-list-devices' -FilePath 'cargo' -ArgumentList @(
        'run',
        '-p', 'coldvox-app',
        '--bin', 'coldvox',
        '--features', $FeatureList,
        '--quiet',
        '--locked',
        '--',
        '--list-devices'
    )
    if ($devices.ExitCode -ne 0) {
        throw "coldvox --list-devices failed with exit code $($devices.ExitCode)."
    }

    $gui = Invoke-LoggedProcess -Name 'coldvox-gui-smoke' -FilePath 'cargo' -ArgumentList @(
        'run',
        '-p', 'coldvox-gui',
        '--quiet'
    )
    if ($gui.ExitCode -ne 0) {
        throw "coldvox-gui smoke failed with exit code $($gui.ExitCode)."
    }

    [pscustomobject]@{
        Preflight = $preflight
        Help = $help
        Devices = $devices
        Gui = $gui
    }
}

function Invoke-Live {
    $smoke = Invoke-Smoke
    Set-RunEnvironment

    Invoke-LoggedProcess -Name 'coldvox-build' -FilePath 'cargo' -ArgumentList @(
        'build',
        '-p', 'coldvox-app',
        '--bin', 'coldvox',
        '--features', $FeatureList,
        '--quiet',
        '--locked'
    ) | Out-Null

    try {
        $live = Invoke-LoggedProcess -Name 'coldvox-live' -FilePath 'cargo' -ArgumentList @(
            'run',
            '-p', 'coldvox-app',
            '--bin', 'coldvox',
            '--features', $FeatureList,
            '--quiet',
            '--locked'
        ) -TimeoutSeconds $RuntimeSeconds
    } finally {
        Copy-RuntimeLog
    }

    if (-not $live.TimedOut -and $live.ExitCode -ne 0) {
        throw "Live runtime exited early with code $($live.ExitCode)."
    }

    @(
        'ColdVox Windows HTTP remote validation'
        "Timestamp: $Timestamp"
        "Repo root: $RepoRoot"
        "Artifact root: $ArtifactRoot"
        "Compose file: $ComposeFile"
        "Config path: $ConfigPath"
        "Health URL: $HealthUrl"
        "Transcriptions URL: $TranscriptionsUrl"
        "Features: $FeatureList"
        "Preflight health response: $($smoke.Preflight.HealthBody)"
        "Preflight transcription response: $($smoke.Preflight.TranscriptionBody)"
        "help exit code: $($smoke.Help.ExitCode)"
        "list-devices exit code: $($smoke.Devices.ExitCode)"
        "gui smoke exit code: $($smoke.Gui.ExitCode)"
        "live exit code: $($live.ExitCode)"
        "live timed out: $($live.TimedOut)"
        "coldvox.log: $(Join-Path $ArtifactRoot 'coldvox.log')"
        "coldvox.log tail: $(Join-Path $ArtifactRoot 'coldvox.log.tail')"
    ) | Set-Content -Path (Join-Path $ArtifactRoot 'summary.txt') -Encoding UTF8

    Write-Ok "Artifacts written to $ArtifactRoot"
}

switch ($Mode) {
    'Preflight' { Invoke-Preflight | Out-Null }
    'Smoke' { Invoke-Smoke | Out-Null }
    'Live' { Invoke-Live | Out-Null }
}
