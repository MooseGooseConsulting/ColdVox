# AGENTS.md

Canonical agent entrypoint for ColdVox.

## Precedence & Source of Truth

When guidance conflicts, use this exact order:
1. The Code and tests
2. `docs/index.md` and the specific documentation it routes you to
3. `README.md`
4. This file

## Workflow & System of Record

1. **Tasks:** `docs/todo.md` is the canonical system of record for tasks. Check it before starting work.
2. **Current State:** `docs/plans/current-status.md` contains the active runtime reality (e.g., Windows/Parakeet focus).
3. **Execution:** Work iteratively. Validate changes locally before claiming success.
4. **Pull Requests:** 
   - All PR descriptions MUST include a "Reading Order" section (e.g., "1. Traits -> 2. Implementation -> 3. Tests") to assist human reviewers.
   - Follow `docs/standards.md` for changelog and metadata rules.

## Working Rules

- Windows is the priority environment.
- The canonical command surface lives in the root `justfile`.
- Prefer crate-scoped Rust commands for iteration; use workspace-wide commands only when needed.
- The canonical Windows-local validation path is `just windows-run-preflight`, `just windows-smoke`, `just windows-live`, and `just test` (see `docs/windows-live-runbook.md`).
- Prefer git worktrees for parallel work under `../.trees/coldvox-{branch-name}`.

## Ask First Boundaries

STOP and ask the user before:
- Force pushes, rebases that rewrite shared history, or branch deletion
- Dependency changes
- Destructive file cleanup outside the immediate task
- Infra, release, or governance changes

## Navigation Routes

- Docs Index: `docs/index.md`
- Runtime/Config: `docs/reference/crates/app.md`
- Validation/Testing: `docs/windows-live-runbook.md`, `docs/domains/foundation/fdn-testing-guide.md`
- Long-term: `docs/plans`, `docs/research`, `docs/todo.md`
