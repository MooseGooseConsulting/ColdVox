---
doc_type: runbook
subsystem: stt
status: active
freshness: current
preservation: preserve
summary: Windows operator path for the canonical Parakeet HTTP/container STT lane — preflight, smoke, and live validation against the parakeet-cpu container on localhost:5092
signals: ['stt', 'windows', 'parakeet', 'http-remote', 'docker', 'runbook']
created: 2026-04-19
last_verified: 2026-05-19
---

# Windows Live Runbook

This is the Windows operator path for the canonical Parakeet HTTP/container lane. For live-capable development, prefer this local containerized HTTP route over in-process/local-model STT: it is explicit, reproducible, and matches the runtime contract used by the Windows validation wrappers.

The checked-in startup config must stay safe for normal development. Use the Parakeet commands below to opt into the live-capable path instead of changing global defaults.

## Prerequisites

- Windows 11
- Docker Desktop running
- The canonical CPU container from `ops/parakeet/docker-compose.yml`
- Optional: NVIDIA GPU only if you want to experiment with the non-canonical GPU comparison profile on port `8200`
- A microphone-capable local Windows machine or hardware-capable runner for optional live microphone validation

## Canonical Backend

ColdVox's first-class Windows backend is the local HTTP Parakeet CPU container:

- Base URL: `http://localhost:5092`
- Health: `GET /health`
- Transcription: `POST /v1/audio/transcriptions`
- Model field: `parakeet-tdt-0.6b-v2`

`config/default.toml` carries the HTTP transport defaults without making the live path implicit. `config/windows-parakeet.toml` is the explicit Windows live profile; it selects `http-remote` and enables `allow_enigo = true` for launcher/live runs.

## Commands

Bring up the preferred local STT backend and check health:

```powershell
just parakeet-up
just parakeet-health
```

Inspect or stop the backend:

```powershell
just parakeet-logs
just parakeet-down
```

Run the local Parakeet HTTP integration validation without GitHub Actions:

```powershell
just parakeet-validate
```

Preflight the container-backed path:

```powershell
just windows-run-preflight
```

Smoke the repo-owned Windows command path:

```powershell
just windows-smoke
```

Run the canonical launcher:

```powershell
just run
```

Run the local Windows test gate:

```powershell
just test
```

Opt into the timed live runtime during the test gate:

```powershell
$env:COLDVOX_RUN_WINDOWS_LIVE = '1'
just test
```

Run the timed live runtime directly:

```powershell
just windows-live
```

The live runtime can run on this local machine or a Windows hardware-capable runner. It starts ColdVox against the Parakeet HTTP container for a bounded interval; it does not require user speech input to pass. Speak into the microphone only for manual quality checks, and record that as a separate operator note/artifact.

## What The Validator Does

`just windows-run-preflight`, `just windows-live`, and `just parakeet-validate` validate the remote/container lane, not the old local-model lane.

The Windows validation wrappers:

1. ensure Docker is reachable
2. bring up `parakeet-cpu`
3. wait for `http://localhost:5092/health`
4. POST `crates/app/test_data/test_1.wav` to `/v1/audio/transcriptions`
5. run the ColdVox smoke or live command path with the `http-remote` feature enabled

`just parakeet-validate` focuses on the backend contract and ignored live integration tests; it brings up `parakeet-cpu`, waits for `/health`, then drives the plugin/app HTTP-remote tests against `crates/app/test_data/test_1.wav`. `just windows-smoke` / `just windows-live` additionally exercise the Windows app launcher path.

## Artifacts

Each Windows wrapper validation run writes artifacts to:

```text
logs/windows-validation/<timestamp>-<mode>/
```

That directory contains, for each wrapper mode:

- captured stdout
- captured stderr
- direct backend health/transcription responses

Additional artifacts are produced only by the `Live` mode:

- `summary.txt`
- copied runtime log files from `logs/coldvox.log`

## Review / Merge Protocol

For this wave, local artifacts are the review gate.

1. Run the relevant local Windows commands and keep the artifact path.
2. Put the exact commands, container assumptions, and artifact path in the PR description.
3. Wait 5 minutes for review comments before merging.
4. Re-run the relevant local gate after addressing review feedback.

CI is not the release gate for this wave.
