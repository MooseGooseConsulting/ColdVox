---
doc_type: reference
subsystem: foundation
status: active
freshness: current
preservation: preserve
domain_code: fdn
last_reviewed: 2026-04-17
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
- `cargo test -p coldvox-stt --lib --no-default-features --features parakeet --locked`
- `cargo test -p coldvox-gui --lib --locked`
- `cargo test -p coldvox-text-injection --lib --locked`
- `cargo test -p coldvox-text-injection --example test_enigo_live --no-run --no-default-features --features enigo --locked`
- `cargo test -p coldvox-app --test settings_test --locked`
- `cargo test -p coldvox-app --test verify_mock_injection_fix --locked`
- `cargo test -p coldvox-app --test golden_master --no-run --no-default-features --features parakeet,silero,text-injection-enigo --locked`
- `just windows-smoke`

## Optional Live GPU Gate

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
just windows-run-preflight
just windows-smoke
just windows-live
```

## Live Prerequisites

- Windows 11
- NVIDIA GPU with working CUDA support
- A downloaded local Parakeet model directory available through either the standard discovery roots or `PARAKEET_MODEL_PATH`

If the Parakeet model is missing, `just windows-run-preflight` and `just windows-live` fail early with a prerequisite error instead of failing later during plugin startup.

## Notes

- The checked-in default config stays on `mock` so tests remain deterministic.
- The Windows live path opts into `config/windows-parakeet.toml`.
- `coldvox-gui` is only a stub smoke target; Tauri GUI work belongs on `tauri-base`.
- The required matrix compiles `golden_master` with the real Windows feature
  set, but it does not treat that test as a required runtime signal yet because
  the fixture is not a reliable Parakeet-on-Windows validation path.
