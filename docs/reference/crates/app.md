---
doc_type: index
subsystem: foundation
status: draft
freshness: stale
preservation: preserve
last_reviewed: 2025-10-19
owners: Documentation Working Group
version: 1.0.0
---

# crate: app (Index)

The application crate does not yet expose a README. Authoritative documentation should be added at [`crates/app/README.md`](../../../crates/app/README.md). Until then, use this index to navigate source entry points.

## Key Entry Points

- [`src/main.rs`](../../../crates/app/src/main.rs)

## Startup STT Selection

- Normal startup discovers [`config/default.toml`](../../../config/default.toml), whose checked-in STT preference is `mock`.
- [`config/plugins.json`](../../../config/plugins.json) is plugin-manager persistence. It is not the primary startup selector and must not silently promote normal startup to `http-remote`.
- The preferred Windows live-capable path is explicit: use [`config/windows-parakeet.toml`](../../../config/windows-parakeet.toml), `COLDVOX_CONFIG_PATH`, or `COLDVOX__STT__PREFERRED=http-remote` through the documented `just`/runbook commands.
- The HTTP transport defaults remain in `config/default.toml` so the explicit Parakeet profile can reuse the canonical `localhost:5092` contract without changing default startup behavior.
