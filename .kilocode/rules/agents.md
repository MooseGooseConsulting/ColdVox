# AGENTS.md

Canonical agent entrypoint for ColdVox.

## Read First

1. [docs/index.md](docs/index.md)
2. [docs/windows-live-runbook.md](docs/windows-live-runbook.md)
3. [docs/reference/crates/app.md](docs/reference/crates/app.md)
4. [docs/domains/foundation/fdn-testing-guide.md](docs/domains/foundation/fdn-testing-guide.md)
5. [README.md](README.md) for high-level project context only

For substantial work, read the relevant crate reference under [docs/reference/crates](docs/reference/crates) and the matching domain docs under [docs/domains](docs/domains).

## Precedence

When guidance conflicts, use this order:

1. Code and tests
2. The task-specific crate/domain docs
3. [docs/index.md](docs/index.md) and the docs it routes you to
4. [README.md](README.md)
5. This file

[docs/architecture.md](docs/architecture.md) is useful for long-range context, but it is not the source of truth for current runtime behavior.

## Repo Truths

- ColdVox is a Rust workspace for audio capture, VAD, STT routing, and text injection.
- Windows is the priority environment.
- `config/default.toml` is the checked-in startup config and currently defaults STT to `mock`.
- `config/plugins.json` is plugin-manager persistence, not the primary startup config.
- `crates/coldvox-gui` exists, but the GUI is still a stub/prototype path.
- The canonical command surface lives in the root [justfile](justfile).
- The canonical Windows-local validation path is `just windows-run-preflight`, `just windows-smoke`, `just windows-live`, and `just test` as documented in [docs/windows-live-runbook.md](docs/windows-live-runbook.md).
- Prefer git worktrees for parallel work under `../.trees/coldvox-{branch-name}`.

## Working Rules

- Prefer crate-scoped Rust commands for iteration; use workspace-wide commands only when needed.
- Validate changes before claiming success. Start with the smallest relevant check, then widen only as needed.
- Keep documentation changes thin and link-heavy; do not turn root agent docs into a second README.
- Update [CHANGELOG.md](CHANGELOG.md) only for user-visible changes, following [docs/standards.md](docs/standards.md).

## Ask First

- Force pushes, rebases that rewrite shared history, or branch deletion
- Dependency changes
- Destructive file cleanup outside the immediate task
- Infra, release, or governance changes

## Useful Routes

- Runtime/config behavior: [docs/reference/crates/app.md](docs/reference/crates/app.md)
- Windows validation commands and artifact flow: [docs/windows-live-runbook.md](docs/windows-live-runbook.md)
- Testing and current test reality: [docs/domains/foundation/fdn-testing-guide.md](docs/domains/foundation/fdn-testing-guide.md)
- Documentation index: [docs/index.md](docs/index.md)
- Documentation policy: [docs/standards.md](docs/standards.md)
- Active documentation backlog: [docs/todo.md](docs/todo.md)
- Longer-term plans and research: [docs/plans](docs/plans) and [docs/research](docs/research)
