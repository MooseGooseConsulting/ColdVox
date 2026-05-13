---
doc_type: plan
subsystem: stt
status: draft
last_reviewed: 2026-05-13
freshness: current
preservation: preserve
summary: "Execution spec for hardening the Parakeet HTTP-remote STT integration path."
signals: ['stt', 'parakeet', 'http-remote', 'integration']
---
# Parakeet HTTP Remote Integration Spec

## Status

- Draft execution spec
- Scope: current repo reality on `feature/windows-gui-shell`
- Goal: validate and harden the configurable HTTP-remote STT path using the canonical Parakeet CPU container profile

## Why This Spec Exists

ColdVox already contains an HTTP-remote STT plugin, canonical runtime config for a local Parakeet CPU service on `http://localhost:5092`, and a container definition under `ops/parakeet/`. However, the current runtime wiring appears inconsistent with the documented/configured remote profile.

This spec defines the next-step work needed to make the HTTP-remote path genuinely configurable and then test it against the real Parakeet container.

## User-Visible Outcome

ColdVox should be able to select the configured HTTP-remote backend and transcribe finalized utterances by sending mono 16 kHz 16-bit WAV audio to a local Parakeet service, without requiring in-process Parakeet or Moonshine.

## Canonical Wave-1 Contract

These values are the current source-of-truth contract in repo docs/config:

- Base URL: `http://localhost:5092`
- Health endpoint: `GET /health`
- Transcription endpoint: `POST /v1/audio/transcriptions`
- Audio payload: mono, 16 kHz, 16-bit WAV
- Request body: multipart form with
  - `file=@audio.wav`
  - `model=parakeet-tdt-0.6b-v2`
  - `response_format=json`
- Response shape: JSON object containing `text`

## Current Repo Evidence

### Active plan and runtime defaults

- `docs/plans/current-status.md`
- `docs/reference/stt-docker-containers.md`
- `config/default.toml`
- `config/plugins.json`
- `ops/parakeet/docker-compose.yml`
- `scripts/run-coldvox.ps1`

### Current implementation files

- `crates/coldvox-stt/src/plugins/http_remote.rs`
- `crates/coldvox-stt/src/plugin.rs`
- `crates/coldvox-stt/src/plugins/mod.rs`
- `crates/app/src/stt/plugin_manager.rs`
- `crates/app/src/lib.rs`
- `crates/app/tests/settings_test.rs`

## Current Mismatches To Resolve

### 1. Preferred plugin ID vs registered HTTP plugin ID

Current config selects:

- `preferred_plugin = "http-remote"`

But the currently registered built-in HTTP factory is:

- `HttpRemotePluginFactory::moonshine_base()`

That factory currently produces a Moonshine-shaped plugin identity rather than the literal canonical `http-remote` identity.

### 2. Registered HTTP factory defaults do not match current canonical contract

The currently registered built-in HTTP factory defaults to Moonshine-oriented values, while canonical docs/config expect Parakeet CPU on port `5092`.

### 3. HTTP plugin initialization does not appear to consume the app’s configured remote settings

The plugin manager initializes the plugin with `TranscriptionConfig::default()`, while the HTTP plugin’s `initialize()` implementation does not currently apply the richer remote settings surface from app configuration.

This means runtime configurability may be narrower in practice than the repo’s config/docs imply.

### 4. Health path and extended request settings are richer in app config than in the current plugin runtime path

App/runtime settings include:

- `health_path`
- `headers`
- `auth`
- `max_audio_bytes`
- `max_audio_seconds`
- `max_payload_bytes`

The next step should verify which of these are already enforced, which are ignored, and which need to be wired through.

## Required Behavior

### Configurability requirements

ColdVox must support:

1. configurable plugin selection
2. configurable HTTP remote endpoint selection
3. configurable health endpoint
4. configurable request metadata where supported (`model`, headers, auth)
5. payload guardrails before request send

### Non-goals for this slice

This spec does **not** require:

- streaming partial transcription over HTTP/WebSocket
- in-process `parakeet-rs` validation
- GUI integration work
- packaging
- replacing all other remote profiles

## Proposed Implementation Order

### Phase A — Contract alignment

1. Make the runtime-configured preferred HTTP remote profile resolvable via the configured preferred ID.
2. Ensure the built-in canonical HTTP remote profile matches the repo’s current Parakeet CPU contract.
3. Make the plugin manager pass the effective remote configuration through to the HTTP plugin runtime path.

### Phase B — Guardrails

4. Honor configured `health_path`.
5. Honor configured headers/auth if present.
6. Enforce configured payload/audio size guardrails before request send.

### Phase C — Real validation

7. Start the canonical Parakeet CPU container.
8. Verify `GET /health`.
9. Verify `POST /v1/audio/transcriptions` using a known WAV file.
10. Run focused Rust tests for the HTTP-remote plugin and app-level selection/config behavior.

## Validation Commands

### Container validation

From repo root:

```powershell
docker compose -f ops/parakeet/docker-compose.yml config --quiet
docker compose -f ops/parakeet/docker-compose.yml up -d
curl http://localhost:5092/health
curl -X POST http://localhost:5092/v1/audio/transcriptions `
  -F "file=@D:/_projects/ColdVox/crates/app/test_data/test_1.wav" `
  -F "model=parakeet-tdt-0.6b-v2" `
  -F "response_format=json"
```

### Rust validation

```powershell
cargo test -p coldvox-stt --features http-remote
cargo test -p coldvox-app --features http-remote
cargo check -p coldvox-app --features http-remote,text-injection
cargo check --workspace --all-targets
```

## Acceptance Criteria

This slice is complete when all of the following are true:

1. The preferred configured HTTP remote backend can be selected without falling back due to ID mismatch.
2. The selected HTTP remote backend uses the intended configured endpoint values.
3. The Parakeet CPU container responds successfully on `GET /health`.
4. A real WAV transcription request to `POST /v1/audio/transcriptions` returns JSON containing non-empty `text`.
5. Focused Rust tests for HTTP remote behavior pass.
6. `cargo check --workspace --all-targets` passes or any pre-existing unrelated failures are clearly documented.

## Risks / Likely Failure Modes

- Current code may still privilege Moonshine-shaped defaults despite newer config/docs.
- The container may be healthy while ColdVox still fails due to selection/config wiring.
- Payload contract differences may only appear under real WAV upload.
- Docker availability or local network policy may block container startup on this machine.

## Recommended Commit Shape

1. `docs(stt): specify parakeet http-remote validation contract`
2. `test(stt): lock configurable http-remote selection behavior`
3. `fix(stt): align canonical http-remote runtime with configured profile`
4. `fix(stt): honor remote health and request settings`
5. `test(stt): validate canonical parakeet container path`

## Immediate Next Step

Run the real container validation sequence against the current repo state first, so any mismatch is captured with concrete evidence before changing code.
