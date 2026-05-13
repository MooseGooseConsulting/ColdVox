---
doc_type: reference
subsystem: developer-workflow
status: active
last_reviewed: 2026-05-13
freshness: current
preservation: preserve
summary: "Developer command reference for common ColdVox build, test, and run commands."
signals: ['commands', 'developer-workflow', 'validation']
---
# Build Commands

## File-scoped (preferred)

```bash
cargo check -p coldvox-app
cargo clippy -p coldvox-audio
cargo test -p coldvox-text-injection
cargo fmt --all -- --check
```

## Workspace (when needed)

```bash
./scripts/local_ci.sh
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo build --workspace --locked
```

## Run

```bash
cargo run -p coldvox-app --bin coldvox
cargo run -p coldvox-app --bin tui_dashboard
cargo run -p coldvox-app --features text-injection,moonshine
```
