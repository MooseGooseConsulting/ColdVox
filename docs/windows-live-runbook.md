---
doc_type: runbook
subsystem: stt
status: active
freshness: current
preservation: preserve
summary: Windows operator path for the canonical Parakeet HTTP/container STT lane — preflight, smoke, and live validation against the parakeet-cpu container on localhost:5092
signals: ['stt', 'windows', 'parakeet', 'http-remote', 'docker', 'runbook']
created: 2026-04-19
last_verified: 2026-04-19
---

# Windows Live Runbook

This is the Windows operator path for the canonical Parakeet HTTP/container lane.

## Prerequisites

- Windows 11
- Docker Desktop running
- The canonical CPU container from `ops/parakeet/docker-compose.yml`
- Optional: NVIDIA GPU if you want to experiment with the non-canonical GPU comparison profile on port `8200`

## Canonical Backend

ColdVox's first-class Windows backend is the local HTTP Parakeet CPU container:

- Base URL: `http://localhost:5092`
- Health: `GET /health`
- Transcription: `POST /v1/audio/transcriptions`
- Model field: `parakeet-tdt-0.6b-v2`

`config/plugins.json` selects `http-remote` for normal startup, and `config/windows-parakeet.toml` forces the same profile for Windows launchers that also need `allow_enigo = true`.

## Commands

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

## What The Validator Does

`just windows-run-preflight` and `just windows-live` now validate the remote/container lane, not the old local-model lane.

They:

1. ensure Docker is reachable
2. bring up `parakeet-cpu`
3. wait for `http://localhost:5092/health`
4. POST `crates/app/test_data/test_1.wav` to `/v1/audio/transcriptions`
5. run the ColdVox smoke or live command path with the `http-remote` feature enabled

## Artifacts

Each validation run writes artifacts to:

```text
logs/windows-validation/<timestamp>-<mode>/
```

That directory contains, for every mode:

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
