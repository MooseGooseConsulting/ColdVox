---
doc_type: reference
subsystem: general
status: draft
freshness: stale
preservation: preserve
last_reviewed: 2025-12-12
owners: Documentation Working Group
version: 1.0.0
---

# Dependency Overview

This document tracks runtime and tooling dependencies for ColdVox.

## Runtime

See `Cargo.toml` in each crate for Rust dependencies. Key runtime dependencies:

- **Audio**: `cpal` (cross-platform audio), `rtrb` (ring buffer)
- **VAD**: `voice_activity_detector` (Silero ONNX VAD)
- **STT**: `parakeet-rs` is the supported Windows live path for this wave and expects a local Parakeet model directory on NVIDIA/CUDA hardware. `pyo3` remains in the tree for older Moonshine-related paths, but it is not the primary Windows validation path.
- **Text Injection**: `enigo`, `atspi`, `wl-clipboard` bindings
- **TUI**: `ratatui`, `crossterm`

For day-to-day local validation, the checked-in default path remains `mock`; live Windows runs opt into `parakeet` through `config/windows-parakeet.toml`.

## Tooling

The tooling and CI sections below remain useful background, but they are not the release gate for the current Windows validation wave. Local Windows validation is the active gate for this pass.

### Security Scanning

ColdVox uses two security tools that run in CI:

#### cargo-audit

Scans `Cargo.lock` for crates with known security vulnerabilities from the [RustSec Advisory Database](https://rustsec.org/).

```bash
# Install
cargo install cargo-audit

# Run
cargo audit
```

#### cargo-deny

Comprehensive dependency linting: licenses, bans, advisories, and sources. Configuration in `deny.toml`.

```bash
# Install
cargo install cargo-deny

# Run all checks
cargo deny check

# Run specific check
cargo deny check licenses
cargo deny check advisories
cargo deny check bans
```

### deny.toml Configuration

The `deny.toml` file in the repository root configures cargo-deny:

- **[licenses]**: Allowed licenses (MIT, Apache-2.0, BSD-3-Clause, etc.)
- **[licenses.private]**: Ignores unpublished workspace crates
- **[bans]**: Crate banning rules (currently empty, logs duplicates as warnings)
- **[advisories]**: Security advisory handling (ignores unmaintained crates with no security impact)
- **[sources]**: Warns on unknown registries or git sources

### Other Tooling

- **rustfmt**: Code formatting (`cargo fmt`)
- **clippy**: Linting (`cargo clippy`)
- **uv**: Python dependency management for STT plugins

## Mixed Rust + Python Tooling (Dec 2025)

- **uv (Python 3.13-ready)**: Use `uv` for Python env + lockfiles; use its global cache to keep CI downloads minimal across runners. Treat `uv lock` as the single source of truth for Python deps; prefer `uv tool install` for CLI tools (ruff, maturin).
- **maturin for packaging**: Build PyO3 wheels with `maturin build -r` and install in CI with `maturin develop` when you need editable bindings. Prefer `uv tool run maturin ...` so we do not rely on system pip. Follow PyO3 0.27 guidance for free-threaded Python 3.13 by enabling the `abi3-py313` or interpreter-specific feature in the binding crates as needed.
- **Rust/Python interface hygiene**: Avoid `pyo3` debug builds in CI; set `PYO3_CONFIG_FILE` only when linking against non-system Python. For embedded Python, ensure `python3-devel` is present on self-hosted runners and keep `extension-module` + `auto-initialize` features constrained to the crates that need them.
- **Shared caching**: Keep `target/` out of VCS; rely on Swatinem `rust-cache` in GitHub Actions plus `uv` global cache for Python wheels.

### sccache (Rust Build Cache)

sccache caches Rust compilation artifacts across builds, significantly reducing rebuild times for unchanged code.

**Installation (one-time on self-hosted runner)**:
```bash
./scripts/setup_sccache.sh
# Or manually: cargo install sccache --locked
```

**CI Integration**: The CI workflow automatically:
1. Checks if sccache is available on the runner
2. If found, starts the sccache server and sets `RUSTC_WRAPPER`
3. If not found, builds proceed normally (no failure)

**Local Development**:
```bash
# Add to ~/.bashrc or ~/.zshrc
export RUSTC_WRAPPER=sccache
export SCCACHE_DIR=~/.cache/sccache

# Check stats
sccache --show-stats
```

**Cache Location**: `~/.cache/sccache` (configurable via `SCCACHE_DIR`)

**Expected Savings**: 30-60% reduction in incremental build times on the self-hosted runner.

## CI Gating Expectations

- **Local Windows validation is the gate for this wave**: use `just test`, `just windows-smoke`, and the opt-in live path on a CUDA-capable Windows machine. GitHub-side checks remain background signal, not the release gate.
- **Hardware-backed E2E remains opt-in**: real-device jobs still belong on self-hosted hardware when we wire them back in, but they are not the blocking gate for the current Windows correction wave.
- **Cache-aware local runs still matter**: warming the local cargo cache before long GPU/device checks keeps the live machine focused on runtime validation instead of repeated compilation.
- **Python/Rust lock discipline still applies**: keep `uv.lock` and `Cargo.lock` in sync with feature flags and PyO3 ABI choices so local validation reflects the committed dependency graph.
