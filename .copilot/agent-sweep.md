# Agent Sweep Ledger

## Branch
- `copilot/add-tauri-and-qt-backend`

## Baseline checks (Linux sandbox)
- Installed `libasound2-dev` for alsa-sys.
- `cargo check` core crates (stt, vad, foundation, telemetry, audio): PASS.
- `cargo fmt --all -- --check`: FAIL — only `crates/coldvox-gui/src-tauri/src/lib.rs`.
- `cargo clippy` core crates: 1 warning — `manual_checked_ops` in `coldvox-audio/src/frame_reader.rs:45`.

## Environment limits
- No Qt6 / Tauri webkit / Python moonshine / audio hardware in sandbox.
- Focus on cheap, pure-Rust crates and docs/tooling.

## Completed tasks
- Fixed rustfmt violations in `coldvox-gui/src-tauri/src/lib.rs`.
- Fixed clippy `manual_checked_ops` in `coldvox-audio/src/frame_reader.rs`.
- Synced `Cargo.lock` (added missing `tokio` dep for `coldvox-gui`).
- Documented `coldvox-gui-qt` in `CHANGELOG.md` and linked it from `docs/reference/crates/coldvox-gui.md`.
- Applied rustfmt to excluded `coldvox-gui-qt` crate.

## Verification results
- `cargo fmt --all -- --check`: PASS.
- `cargo fmt --manifest-path crates/coldvox-gui-qt/Cargo.toml -- --check`: PASS.
- `cargo clippy -- -D warnings` (buildable crates): PASS.
- Buildable crate tests (audio/stt/vad/text-injection/foundation/telemetry): PASS.

## Blocked / deferred
- `coldvox-app` and `coldvox-vad-silero` (default features): `ort-sys` build downloads onnxruntime → blocked by sandbox network.
- GUI crates (Tauri/Qt) full build: missing system deps (webkit2gtk / Qt 6).

## Remaining useful work (next agent)
- Run full `cargo clippy --workspace -- -D warnings` and `cargo test --workspace` on a runner with ALSA + onnxruntime + Qt/webkit available.
- Consider a dedicated `docs/reference/crates/coldvox-gui-qt.md` index doc matching repo doc conventions.

## Assumptions
- CI gate is `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo fmt --all -- --check` per justfile.
