---
doc_type: status
subsystem: general
status: active
---

# Current Product Direction & Reality

Audited April 17, 2026.

## Branch Strategy

- `main` is the production trunk.
- `tauri-base` is the GUI integration trunk. Work touching Tauri, GUI integration, or Windows GUI validation should target `tauri-base` first.
- Do not open `tauri-base` -> `main` promotion PRs unless that promotion is explicitly requested.
- The older Qt remediation line from closed PR `#389` is not a valid base for new work.

## Windows Runtime Reality

- The checked-in default startup config remains deterministic: `config/default.toml` uses `mock`.
- The supported Windows live path is `parakeet` on NVIDIA/CUDA hardware via `config/windows-parakeet.toml`.
- Live Windows runs must supply a local Parakeet model directory through `PARAKEET_MODEL_PATH`.
- `config/plugins.json` is persistence for plugin selection state, not the primary operator-facing startup config.

## Validation Reality

- The authoritative gate for the Windows runtime path is local Windows validation, not CI.
- `just windows-run-preflight` checks GPU/CUDA prerequisites and the local Parakeet model requirement.
- `just windows-smoke` validates CLI help, device enumeration, and the GUI stub smoke path.
- `just test` runs the Windows-safe required matrix on Windows and keeps the live runtime behind `COLDVOX_RUN_WINDOWS_LIVE=1`.
- Validation artifacts are written under `logs/windows-validation/<timestamp>-<mode>/`.

## GUI Reality

- `coldvox-gui` is not the shipped Windows path.
- `cargo run -p coldvox-gui` is kept only as a stub smoke check.

## Known Live Blocker

- A Windows machine without a downloaded local Parakeet model directory will fail `just windows-run-preflight` and `just windows-live` until `PARAKEET_MODEL_PATH` is set or the model is placed in the expected local cache path.
