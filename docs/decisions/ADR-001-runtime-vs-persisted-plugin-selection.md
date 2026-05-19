# ADR-001: Runtime vs Persisted Plugin Selection

- **Status:** ACCEPTED
- **Date:** 2026-05-19

## Context

`SttPluginManager` owns the active STT plugin selection at runtime.
Plugin selection is also persisted to `config/plugins.json` so it
survives restarts. Before commit `a145f04` (2026-05-19), a single
method handled both responsibilities: it applied the selection
in-memory **and** rewrote `plugins.json` to disk on every call. This
had two adverse effects:

1. **App startup silently overwrote the user's persisted config.**
   Each process bootstrap computed a `PluginSelectionConfig` from the
   current settings and called the apply-and-persist method — even
   when startup was merely *reflecting* whatever was already on disk.
   Users who hand-edited `plugins.json` to pin a specific plugin would
   see their edits discarded on the next boot.

2. **The "selection" concept conflated two semantics.** A UI flow
   that says *"the user just picked HTTP-remote, persist that choice"*
   and a startup flow that says *"apply whatever is currently
   configured"* are not the same operation. Treating them identically
   made it impossible to route plugin selection from startup without
   persistence side effects.

## Decision

We split `SttPluginManager` plugin-selection application into two
public methods with explicit, name-encoded semantics:

- `set_selection_config(cfg)` — apply selection in-memory **and**
  persist to `config/plugins.json` on disk. Call this from UI flows
  and explicit config-change commands.
- `set_runtime_selection_config(cfg)` — apply selection in-memory
  **only**. Do **not** rewrite `plugins.json`. Call this from app
  startup paths and any other site that is merely reflecting state
  that is already persisted (or that ought to be ephemeral).

Both methods share an internal
`apply_selection_config(cfg, persist: bool)` helper. The boolean is
encoded in the method choice at the call site, so a caller cannot
accidentally pick the wrong persistence behavior without choosing a
clearly-different method name.

## Drivers

- **User intent preservation.** `config/plugins.json` is user-owned
  data. The runtime should not rewrite it as a side effect of starting
  up.
- **Call-site clarity.** A method name should make persistence
  behavior obvious without reading the implementation.
- **Boolean-parameter avoidance.** A single
  `set_selection_config(cfg, persist)` is technically equivalent but
  easy to misuse — the wrong default at the wrong call site silently
  regresses behavior.
- **Forward compatibility.** Future selection sources (env override,
  CLI flag, programmatic test setup) can use the runtime variant
  without contaminating persisted state.

## Alternatives Considered

- **Single method with `persist: bool` parameter.** Rejected: easy to
  pass the wrong default. The compiler cannot distinguish *"I forgot
  to think about persistence"* from *"I deliberately want to skip it."*
- **Always persist (the pre-`a145f04` behavior).** Rejected: this is
  the root cause of the bug this ADR closes.
- **Never persist; require an explicit separate `save()` call.**
  Rejected: UI flows legitimately want one-call atomic persistence.
  Splitting apply and save creates a new failure mode where in-memory
  and on-disk state drift when `save()` is missed.
- **Watch `plugins.json` for external edits with mtime-based reload.**
  Rejected as out of scope for this decision; orthogonal to the
  apply-vs-persist split.

## Consequences

**Positive**

- `config/plugins.json` is preserved across restarts unless the user
  explicitly changes selection via a UI/config path that calls
  `set_selection_config`.
- Startup, tests, and programmatic selection updates all use
  `set_runtime_selection_config` and have zero persistence side
  effects.
- The method names themselves document the contract.

**Negative**

- Two methods to maintain instead of one. Minor.
- Call sites must pick deliberately. This is a feature, not a cost.

**Risks**

- A new call site that *should* persist could accidentally call the
  runtime variant. Mitigation: code review; the method names are
  self-describing.
- A future refactor might collapse the two methods back into one
  parameterized call without understanding this rationale.
  Mitigation: this ADR.

**Follow-ups**

- None directly. The companion decision about *which sources of
  selection win when they conflict* is captured in
  [ADR-002](ADR-002-stt-settings-precedence.md).

## References

- Code: `crates/app/src/stt/plugin_manager.rs` —
  `set_selection_config`, `set_runtime_selection_config`,
  `apply_selection_config`.
- Code: `crates/app/src/runtime.rs` — `start` calls
  `set_runtime_selection_config` from app startup.
- Commits:
  - `a145f04` (2026-05-19) — introduced the split.
  - `c36621f` (2026-05-19) — regression that motivated documenting the
    runtime contract; reverted by `2982706`.
- Related: [ADR-002](ADR-002-stt-settings-precedence.md).
