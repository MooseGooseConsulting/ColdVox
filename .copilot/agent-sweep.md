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
- (in progress)

## Blocked / deferred
- GUI crates (Tauri/Qt) full build: missing system deps.

## Assumptions
- CI gate is `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo fmt --all -- --check` per justfile.
