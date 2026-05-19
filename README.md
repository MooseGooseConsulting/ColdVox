# ColdVox

> Internal alpha. This repository is under active cleanup and the Windows path is still being hardened.

ColdVox is a Rust voice pipeline: audio capture -> VAD -> STT -> text injection.

## Current Reality

- The checked-in default config stays deterministic and test-friendly: `config/default.toml` starts with the `mock` STT path.
- The supported Windows live path is the local Parakeet HTTP container profile in `config/windows-parakeet.toml`.
- The current `coldvox-gui` binary is only a stub smoke check; Tauri GUI work belongs on `tauri-base`.
- Local Windows validation is the release gate for the Windows runtime path.

## Windows Live Path

Prerequisites:

- Windows 11
- NVIDIA GPU with working CUDA support
- A downloaded Parakeet model directory that is either exposed through `PARAKEET_MODEL_PATH` or available in the normal shared/local discovery roots

Commands:

```powershell
just windows-run-preflight
just windows-smoke
just test
```

To opt into the live runtime during the test gate:

```powershell
$env:COLDVOX_RUN_WINDOWS_LIVE = '1'
just test
```

To run the live validation directly:

```powershell
just windows-live
```

Validation artifacts are written under `logs/windows-validation/<timestamp>-<mode>/`.

## Developer Commands

```powershell
cargo check -p coldvox-app
cargo fmt --all -- --check
```

For the detailed Windows operator path, see [`docs/windows-live-runbook.md`](docs/windows-live-runbook.md).
For agent/developer repo truth, see [`AGENTS.md`](AGENTS.md) and [`docs/plans/current-status.md`](docs/plans/current-status.md).
