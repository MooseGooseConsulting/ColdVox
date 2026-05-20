# ADR-002: STT Settings Precedence

- **Status:** ACCEPTED
- **Date:** 2026-04-18 (effective in code), 2026-05-19 (documented)

## Context

The runtime plugin selection for STT can be sourced from two
configuration surfaces:

1. **TOML startup config** (e.g. `config/default.toml`,
   `config/windows-parakeet.toml`) — the `[stt]` table's `preferred`
   field and `fallbacks` list.
2. **JSON persistence file** (`config/plugins.json`) — the
   `preferred_plugin` field and `fallback_plugins` list.

When both surfaces specify a value, one must win. Prior to commit
`f811f72` (2026-04-18), `plugins.json` won. After `f811f72`,
`[stt].preferred` from TOML wins.

The precedence flip was not documented at the time. It later turned
out to be the mechanism that commit `c36621f` (2026-05-19) exploited
to silently make `mock` the runtime default — by setting
`preferred = "mock"` in `config/default.toml`, `c36621f` overrode the
`http-remote` selection in `plugins.json` without any operator action.
The recovery (`2982706`) reverted `c36621f` but the precedence rule
itself is correct as designed; it needs to be made explicit.

## Decision

The runtime plugin selection resolves in this order, first-non-empty wins:

1. **Explicit TOML override.** If a startup-config TOML sets
   `[stt].preferred` to a non-`None` value, that value is the runtime
   preferred plugin and `config/plugins.json` is **not** consulted for
   `preferred_plugin`.

2. **Explicit `COLDVOX_CONFIG_PATH`.** If the `COLDVOX_CONFIG_PATH`
   environment variable is set, the discovery path for `plugins.json`
   short-circuits to `None` (the TOML's `[stt]` table is authoritative
   — even if `[stt].preferred` is `None`, we do not fall back to
   `plugins.json`).

3. **`config/plugins.json`.** If TOML did not provide a preferred
   plugin and `COLDVOX_CONFIG_PATH` is not set, `plugins.json` is
   loaded via `load_canonical_plugin_selection_config()` and its
   `preferred_plugin` becomes the runtime preferred.

4. **No selection.** If none of the above produced a value, plugin
   selection is `None` and the manager falls back to whatever the
   registry can fail over to.

For the fallback list, the rule is analogous: if `[stt].fallbacks`
is non-empty in TOML, it wins; otherwise `plugins.json`'s
`fallback_plugins` is used.

The checked-in `config/default.toml` does **not** set
`[stt].preferred`, so the default startup path falls through to
`config/plugins.json` which ships with
`preferred_plugin = "http-remote"`.

## Drivers

- **Explicit TOML configs must be authoritative when the operator
  chooses them.** Pointing at `windows-parakeet.toml` should pin a
  plugin reliably, regardless of what stale `plugins.json` might be
  lying around from previous experiments.
- **The default startup must not require operators to maintain TOML
  redundancy.** Leaving `[stt].preferred` unset in `default.toml` and
  letting `plugins.json` be authoritative for the default case
  minimizes config sprawl.
- **Precedence must be one-directional and obvious.** Field-by-field
  merging is unintuitive; "TOML wins iff set" is easy to reason about.

## Alternatives Considered

- **`plugins.json` always wins (pre-`f811f72` behavior).** Rejected:
  defeats explicit TOML configs. An operator pointing at
  `windows-parakeet.toml` cannot reliably pin a plugin if
  `plugins.json` silently overrides.
- **Merge field-by-field with union semantics.** Rejected: ambiguous
  and surprising. Two configs that each look correct in isolation can
  combine into a third config no one intended.
- **TOML always wins, even when fields are missing or `None`.**
  Rejected: forces every TOML to explicitly opt out of `plugins.json`
  or re-state every field. Excessive ceremony for the common case.

## Consequences

**Positive**

- Explicit operator intent (selecting a startup TOML) is preserved.
- The default startup path is minimal: TOML carries non-STT settings;
  `plugins.json` carries plugin selection.
- A single file change can pin or unpin a plugin without touching the
  other surface.

**Negative**

- **Tripwire: setting `preferred` in any TOML loaded as startup
  config silently overrides `plugins.json`.** This is the mechanism
  `c36621f` exploited. The AGENTS.md Working Rule
  *"Canonical STT default changes require an ADR"* is the primary
  procedural mitigation.

**Risks**

- A future agent edits a TOML to add `preferred = "<something>"`
  without realizing it overrides `plugins.json`. Mitigations: AGENTS.md
  Working Rule; this ADR; future structural lint (tracked in
  `docs/todo.md`).
- An operator copies a TOML from another environment that has
  `preferred` set, unknowingly carrying that setting forward into a
  context where `plugins.json` was supposed to win. Mitigation: TOML
  comments at the `[stt]` table head pointing to this ADR.

**Follow-ups**

- Tracked in [docs/todo.md](../todo.md): add an inline comment above
  `[stt]` in `config/default.toml` linking back to this ADR.

## References

- Code: `crates/app/src/lib.rs` —
  `build_runtime_plugin_selection_with_overrides` implements the
  precedence chain.
- Code: `crates/app/src/lib.rs` —
  `discover_plugin_selection_config_path` implements the
  `COLDVOX_CONFIG_PATH` short-circuit.
- Commits:
  - `f811f72` (2026-04-18) — flipped precedence so TOML wins.
  - `c36621f` (2026-05-19) — exploited the precedence to flip the
    runtime default to `mock`; reverted by `2982706`.
- Related: [ADR-001](ADR-001-runtime-vs-persisted-plugin-selection.md).
