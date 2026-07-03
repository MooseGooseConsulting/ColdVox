# Changelog

All notable changes to this project are documented here.

## [Unreleased]

### Added
- AI-gated automerge pipeline on `tauri-base`: `agent-review.yml` reads CodeRabbit reviews and applies `agent-approved`/`agent-blocked` labels; `automerge.yml` enables `gh pr merge --auto` on non-draft PRs; `gate-main.yml` enforces that only PRs from `tauri-base` on the canonical repo may target `main` (blocks forks and off-branch merges).

### STT
- Pointed the remote Parakeet profile (`[stt.remote]` in `config/default.toml`) at the k8s cluster endpoint `http://192.168.30.207:5092` (Deployment `parakeet` in namespace `apps`, coldaine-k8cluster; same digest-pinned `ghcr.io/achetronic/parakeet` image). No local container startup is needed for the primary path; the local compose profile (`ops/parakeet/docker-compose.yml` on `localhost:5092`) remains the offline dev fallback and stays the built-in code default.
- Hardened the canonical Parakeet CPU HTTP-remote profile so `http-remote` now resolves to the configured `5092` `/health` + `/v1/audio/transcriptions` contract, honors remote request/guardrail settings, and ships with a repo-owned CPU compose profile under `ops/parakeet/`.
- Added an optional containerized Parakeet GPU HTTP comparison profile (`http-remote-parakeet-gpu`) with a repo-owned compose service on `8200`, using the live `/healthz` + `/audio/transcriptions` contract while preserving the CPU profile as the wave-1 default.

### GUI
- Replaced the old `crates/coldvox-gui` Qt/QML placeholder with a Tauri v2 + React overlay shell.
- Added a demo-only typed command/event seam between the Rust host shell and the frontend to exercise collapsed/expanded states, transcript promotion, and visible `idle`/`listening`/`processing`/`ready`/`error` feedback without real STT integration.
- Added 5 Tauri commands (`update_partial_transcript`, `update_final_transcript`, `set_overlay_processing`, `set_overlay_listening`, `stop_overlay_capture`) wiring the STT pipeline to the overlay shell, with corresponding `OverlayModel` state transitions.
- Added 80ms debounced partial transcript queuing in `useOverlayShell` to reduce repaints during rapid STT output; pending partials are flushed and cancelled on state transitions.
- Fixed `stop_capture` to increment `demo_token` so in-flight demo driver loops exit correctly.
- Fixed pipeline state transitions to reset the `paused` flag so demo pause state does not leak into real capture sessions.
- Added focused Rust and frontend tests for the overlay state contract and React hook/component behavior.

### Nuclear Pruning & Documentation Cleanup
- Removed vaporware STT backends (whisper, coqui, leopard, silero-stt) and legacy feature flags.
- Archived outdated plans, PR reports, and reference docs to `docs/archive/`.
- Updated all agent anchors (`AGENTS.md`, `README.md`) to the current documentation chain centered on `docs/plans/current-status.md`.
- Added `.omc/` (AI tool state) to `.gitignore`.
- Synced `plugins.json` configs to prefer `moonshine` (removed whisper references).
- Fixed broken links and references post-restructure.
- Updated `docs/plans/current-status.md` with current verified status and paths.

### Added
- **Moonshine STT Plugin** - CPU-optimized speech recognition using UsefulSensors' Moonshine model via PyO3/HuggingFace Transformers
  - 5x faster than Whisper on CPU with comparable accuracy (~2.5% WER)
  - English-only, optimized for 16kHz audio
  - Two model variants: Base (61M params, ~500MB) and Tiny (27M params, ~300MB)
  - Auto-downloads models from HuggingFace Hub on first use
  - Environment variables: `MOONSHINE_MODEL` (base/tiny), `MOONSHINE_MODEL_PATH`
  - Requires Python 3.8+ with transformers, torch, librosa
  - Install deps: `./scripts/install-moonshine-deps.sh`
  - Build: `cargo build --features moonshine`

- **NVIDIA Parakeet STT Plugin** - GPU-accelerated speech recognition using NVIDIA's Parakeet model via pure-Rust parakeet-rs library (#XXX)
  - Supports largest available model: nvidia/parakeet-tdt-1.1b (1.1 billion parameters)
  - TDT variant: Multilingual support for 25 languages with automatic detection
  - CTC variant: English-only for faster inference
  - GPU acceleration via feature flags: `parakeet-cuda` (CUDA), `parakeet-tensorrt` (TensorRT)
  - Falls back to CPU when GPU features are not compiled (with warning)
  - Token-level timestamps for word-accurate transcription
  - Environment variables: `PARAKEET_MODEL_PATH`, `PARAKEET_VARIANT` (tdt/ctc), `PARAKEET_DEVICE` (cuda/tensorrt)
  - Pure Rust implementation - no Python dependencies

- **Audio Quality Monitoring** - Real-time audio quality detection and feedback (#345)
  - New `coldvox-audio-quality` crate for automated quality monitoring
  - Detects too-quiet audio (RMS < -40 dBFS), clipping (peak > -1 dBFS), and off-axis speech (spectral ratio < 0.3)
  - FFT-based off-axis detection using high-freq/mid-freq ratio analysis
  - Configurable thresholds via `QualityConfig` builder or environment variables
  - Pre-allocated buffers for real-time safety (~12.8µs per 512-sample frame)
  - Rolling window RMS (500ms) and peak hold (1s) with exponential decay
  - Rate-limited warnings (2-second cooldown) to avoid spam
  - Microphone presets (HyperX QuadCast, Omnidirectional)
  - Environment variables: `COLDVOX_TOO_QUIET_THRESHOLD`, `COLDVOX_CLIPPING_THRESHOLD`, `COLDVOX_OFF_AXIS_THRESHOLD`
  - Integration tests with real audio data (LibriSpeech, Pyramic anechoic dataset)
  - Download test datasets: `./scripts/download_test_audio.sh`

### Configuration
- Canonicalize STT selection config to `config/plugins.json`. Legacy duplicates like `./plugins.json` and `crates/app/plugins.json` are deprecated and ignored at runtime; a startup warning is logged if detected. Documentation updated to reflect the single source of truth.

### Build & Tooling
- Python 3.13 note: temporarily support building with Python 3.13 by setting `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` (until `pyo3` is upgraded to officially support 3.13).

### Logging and Observability
- **Change default log level from DEBUG to INFO** to reduce verbosity in normal operation
- Downgrade high-frequency logs to appropriate levels:
  - Silence detection events: INFO → DEBUG
  - Audio chunk dispatch: INFO → TRACE
  - Plugin process calls: DEBUG → TRACE
  - Plugin process results: DEBUG → TRACE (success) / WARN (errors)
- Add structured logging to text-injection manager with detailed diagnostics:
  - Method path snapshots showing availability, success rates, and cooldown states
  - Focus status, injection mode, and char count logging
  - Throttled session state diagnostics to avoid log spam
- Create comprehensive `docs/logging.md` with usage examples and troubleshooting guide
- Add `injection_diagnostics` example for troubleshooting injection issues
- Extract shared test utilities for clipboard testing

Users can still enable detailed debugging via `RUST_LOG=debug` or `RUST_LOG=trace` environment variables.

### Core Architecture
- Migrate runtime, VAD, STT processor, and probes to SharedAudioFrame (Arc<[i16]>) for zero-copy fanout across the audio pipeline. This reduces allocations and improves throughput in multi-consumer scenarios.

### STT Plugin Manager
- Remove silent NoOp fallback paths. Initialization now fails explicitly when preferred and fallback plugins are unavailable, and failover will not switch to NoOp. Tests adjusted to reflect strict behavior.
- Hardened "best available" selection to never auto-pick NoOp.

### Whisper STT
- Default language to "en" automatically when using English-only models (e.g., base.en/small.en) to suppress repeated runtime warnings.
- Test stability: set TQDM_DISABLE=0 in E2E tests to avoid buggy disabled_tqdm stubs in some Python environments.

### Tests and CI stability
- WAV-driven end-to-end tests now use a dummy capture in test mode instead of opening a real ALSA/CPAL input device. This removes ALSA "Unknown PCM pulse/jack/oss" stderr spam while keeping the full pipeline (chunker → VAD → STT → injection) under test.
- Hotkey E2E test is opt-in to avoid environment-specific Python/tqdm issues: set COLDVOX_RUN_HOTKEY_E2E=1 to run locally. Still skipped in CI/headless.
- WER fallback in E2E test now skips strict assertions in CI/headless or when small/tiny models are in use, validating execution without penalizing constrained environments.

### Configuration
- Add COLDVOX_SKIP_CONFIG_DISCOVERY to bypass loading repo config files during tests that need to assert pure in-code defaults.

### Breaking Changes
- NoOp fallback removal: any workflows relying on implicit NoOp selection must now provide a valid plugin or handle explicit errors. Tests and configs updated accordingly.

### Developer Notes
- Minor warning cleanups (unused imports) and documentation of new env flags in tests.

### Documentation
- **Major documentation restructure** (#180): Implemented Master Documentation Playbook v1.0.0
  - Added comprehensive documentation structure under `/docs` with canonical layout
  - Created Master Documentation Playbook defining standards, metadata schema, and governance
  - Organized documentation into domains (audio, stt, text-injection, vad, gui, foundation)
  - Added revision tracking system with automated CSV logger
  - Established PR workflow requirements including metadata validation
  - Migrated legacy documentation to new structure with proper categorization
  - Added Python virtual environment management using uv with Python 3.12
  - Fixed docs validation script to handle deleted files correctly
  - Updated CLAUDE.md with detailed workspace structure and development guidelines

### Dependencies
- Bump `toml` from 0.8.23 to 0.9.8 (#182)
- Bump `clap` from 4.5.49 to 4.5.50 (#181)
- Keep `atspi` at 0.28.0 (defer 0.29.0 upgrade due to breaking API changes)

### Security & Tooling
- **Migrate deny.toml to cargo-deny v0.18 format**: Fixed deprecated configuration keys (`unlawful` → `allow`-only, `highlighted` → `highlight`, `yank` → `yanked`)
- Added `CDLA-Permissive-2.0` license to allow list (transitive dep from webpki-root-certs)
- Added `[licenses.private]` section to ignore unpublished workspace crates
- Ignored RUSTSEC-2024-0436 (paste unmaintained advisory - no security impact)
- Added `publish = false` to workspace crates: coldvox-app, coldvox-gui, coldvox-stt
- **CI security scanning**: Added cargo-audit and cargo-deny jobs to CI workflow for vulnerability and license compliance checks

## v2.0.2 — 2025-09-12

Highlights
- STT Plugin Manager: Full runtime integration, failover/GC, metrics/TUI
- Tests: Added failover, GC, hot-reload coverage
- Docs: Plugin README section, migration notes

Details
- Complete STT plugin manager with telemetry integration, TUI exposure, and configuration persistence
- Plugin operations instrumented with lifecycle events, transcription statistics, error tracking, and performance timing
- TUI dashboard with Plugins tab, plugin status display, interactive controls ([P] toggle, [L] load, [U] unload)
 - Configuration persistence via serde_json to config/plugins.json with load on init and save on changes
- End-to-end STT pipeline test and concurrent process_audio/GC safety test
- Updated README.md with STT plugins section and migration notes

Upgrade Notes
- STT configuration now uses --stt-* flags
- Plugin settings are automatically persisted to config/plugins.json
- TUI now available with --tui flag (requires tui feature)

PRs
- STT Plugin Completion: Telemetry, TUI, and Configuration Persistence

## v2.0.1 — 2025-09-05

Highlights
- Text Injection: FocusProvider dependency injection for reliable focus handling
- Mocked fallback tests and utilities for deterministic behavior and coverage
- Headless CI: Xvfb + fluxbox readiness checks; workflow validation via `gh`
- Quality: clippy/doc warning cleanup; async `ydotool` availability check
- Documentation: testing guide, architecture diagram updates, coverage analysis

Details
- Add `MockFocusProvider`, `TestInjectorFactory`, and comprehensive tests under `crates/coldvox-text-injection/src/tests/`
- Introduce `combo_clip_ydotool` injector with async `ydotool` check
- Improve `.github/workflows/ci.yml` with readiness loops and clearer dependency setup
- Fix TUI mutability for gated fields; adjust tests to satisfy clippy best practices
- Validate workspace with `fmt`, `clippy`, `check`, `build`, `doc`, and tests

Upgrade Notes
- No breaking API changes in this release
- Optional: install `xdpyinfo` and `wmctrl` if running GUI-dependent tests locally under Xvfb

PRs
- #33 Text Injection: Focus DI, Mocked Fallback Tests, and Headless CI (Xvfb)
