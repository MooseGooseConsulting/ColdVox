---
doc_type: reference
subsystem: foundation
status: active
freshness: current
preservation: preserve
domain_code: fdn
last_reviewed: 2026-05-19
owners: Documentation Working Group
version: 1.0.0
---

# Testing Guide

## Current Gate

The authoritative gate for the Windows runtime path is local Windows validation. CI is supporting signal, not the release gate.

On Windows, `just test` runs the required Windows-safe matrix. It does not call `cargo test --workspace --locked`, because the wider workspace still pulls in non-Windows members that are not a useful Windows correctness signal.

## Required Windows Matrix

`just test` runs:

- `cargo test -p coldvox-foundation --lib --locked`
- `cargo test -p coldvox-audio --lib --locked`
- `cargo test -p coldvox-vad --lib --locked`
- `cargo test -p coldvox-telemetry --lib --locked`
- `cargo test -p coldvox-stt --lib --no-default-features --features http-remote --locked`
- `cargo test -p coldvox-gui --lib --locked`
- `cargo test -p coldvox-text-injection --lib --locked`
- `cargo test -p coldvox-text-injection --example test_enigo_live --no-run --no-default-features --features enigo --locked`
- `cargo test -p coldvox-app --lib --features http-remote --locked`
- `cargo test -p coldvox-app --test settings_test --locked`
- `cargo test -p coldvox-app --test verify_mock_injection_fix --locked`
- `cargo test -p coldvox-app --test golden_master --no-run --features http-remote,text-injection-enigo --locked`
- `just windows-smoke`

## Optional Live Runtime Gate

The live runtime is optional during the default test gate and is controlled by one opt-in variable:

```powershell
$env:COLDVOX_RUN_WINDOWS_LIVE = '1'
just test
```

That opt-in adds `just windows-live` to the end of the Windows test matrix.
It also runs the live Enigo example before the runtime validation wrapper so the
Windows injector path is exercised, not just compiled.

## Direct Validation Commands

```powershell
just parakeet-up
just parakeet-health
just parakeet-validate
just windows-run-preflight
just windows-smoke
just windows-live
```

`just parakeet-validate` is the local, GitHub-Actions-free validation path for the canonical Parakeet HTTP container. It brings up `parakeet-cpu`, waits for `http://localhost:5092/health`, and runs the ignored HTTP-remote live integration tests against `crates/app/test_data/test_1.wav`.

## Live Prerequisites

- Windows 11
- Docker Desktop running
- The `parakeet-cpu` service from `ops/parakeet/docker-compose.yml` on `http://localhost:5092`
- A microphone-capable local machine or hardware-capable runner only when manually exercising live microphone behavior

The Parakeet HTTP/container path owns model provisioning inside the Docker volume, so `PARAKEET_MODEL_PATH` is not required for the canonical Windows live path. The old in-process/local-model Parakeet path remains outside this gate.

## Notes

- The checked-in default config stays on `mock` so tests remain deterministic.
- The preferred live-capable STT route is the containerized Parakeet HTTP path on `localhost:5092`, selected explicitly through `config/windows-parakeet.toml` and the `parakeet-*` / `windows-*` just recipes.
- Automated validation does not require user speech input; manual microphone quality checks should be documented separately with their artifact path.
- `coldvox-gui` is only a stub smoke target; Tauri GUI work belongs on `tauri-base`.
- The required matrix compiles `golden_master` with the real Windows feature
  set, but it does not treat that test as a required runtime signal yet because
  the fixture is not a reliable Parakeet-on-Windows validation path.
