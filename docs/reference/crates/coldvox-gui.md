---
doc_type: index
subsystem: gui
status: draft
freshness: current
preservation: preserve
last_reviewed: 2026-03-29
owners: Documentation Working Group
version: 1.0.0
---

# crate: coldvox-gui (Index)

Current GUI direction lives in [`docs/plans/current-status.md`](../../plans/current-status.md).

The crate-level status note lives in [`crates/coldvox-gui/README.md`](../../../crates/coldvox-gui/README.md).

## Key Entry Points

- Frontend app shell: [`package.json`](../../../crates/coldvox-gui/package.json)
- Frontend React entry: [`src/App.tsx`](../../../crates/coldvox-gui/src/App.tsx)
- Frontend Tauri bridge hook: [`src/hooks/useOverlayShell.ts`](../../../crates/coldvox-gui/src/hooks/useOverlayShell.ts)
- Rust package manifest: [`src-tauri/Cargo.toml`](../../../crates/coldvox-gui/src-tauri/Cargo.toml)
- Rust shell entry: [`src-tauri/src/lib.rs`](../../../crates/coldvox-gui/src-tauri/src/lib.rs)
- Rust overlay model: [`src-tauri/src/state.rs`](../../../crates/coldvox-gui/src-tauri/src/state.rs)

## Alternative Backends

An alternative Qt 6 + QML overlay backend lives in [`crates/coldvox-gui-qt`](../../../crates/coldvox-gui-qt/README.md). It is intentionally excluded from the root Cargo workspace so `cargo check --workspace` still succeeds on machines without Qt installed.
