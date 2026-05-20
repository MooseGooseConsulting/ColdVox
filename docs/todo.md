---
doc_type: standard
subsystem: general
status: draft
freshness: stale
preservation: preserve
last_reviewed: 2026-03-29
owners: Documentation Working Group
version: 1.0.0
---

# Documentation Todo Backlog

## Epic: Test OS Scoping (High Priority)
- [x] Scope text-injection integration tests to fix `cargo test` failures on Windows. ([plan](./plans/current-status.md))

## Epic: Documentation Migration

- [x] Establish canonical directory skeleton and enforcement tooling (Phase 1).
- [ ] Migrate existing Markdown into canonical structure (Phase 2–3).
  - [ ] Align GUI documentation with the Windows-first ColdVox_Mini carryover path ([plan](./plans/current-status.md)).
  - [ ] Document CI runner readiness requirements ([spec](./tasks/ci-runner-readiness-proposal.md)).
- [ ] Normalize content metadata and retention banners (Phase 3 follow-up).
- [ ] Cross-link tasks and specs per playbook policy (Phase 4).
- [ ] Finalize documentation governance updates in root README and standards.

## Epic: Testing Strategy
- [x] Develop playbook for debugging test failures with LLMs ([playbook](./playbooks/testing/llm-test-debugging-playbook.md)).

## Epic: tauri-base STT Rescue Follow-ups (PR-B, post-merge)

These items came out of the 2026-05-19 rescue of commit `c36621f`. They are scoped for a follow-up PR after PR-A merges. Tracked here so the recovery doesn't lose context.

- [ ] **Audit-3** — triage and delete the abandoned local + remote branches surfaced in the rescue inventory. Cross-reference each against open and merged PRs first; never delete without confirming the work is captured elsewhere.
- [ ] **Audit-5** — evaluate a structural lint that asserts the AGENTS.md *"Canonical STT default changes require an ADR"* Working Rule. Design it as per-violation comment + on-disk ADR-N file existence check (a previous attempt that gated globally on *any* ADR file existing was scrapped because it was bypassable).
- [ ] **Observability** — emit a `stt.plugin_selection` tracing event at app startup carrying the resolved `preferred` plugin and the *source* (TOML `[stt].preferred` / `plugins.json` / `None`). Makes the [ADR-002](./decisions/ADR-002-stt-settings-precedence.md) precedence resolution observable post-mortem instead of having to reason from code.
- [ ] **Config hygiene** — add an inline TOML comment above `[stt]` in `config/default.toml` pointing to [ADR-002](./decisions/ADR-002-stt-settings-precedence.md) so the precedence tripwire is visible to anyone editing the file.
- [ ] **.gitignore** — add `.omc/`, `compile_errors.txt`, `prs*.txt` so working files from agent sessions stop polluting `git status`.
- [ ] **Third copy** — `docs/repo/copilot-instructions.md` is a 12.5K third copy of agent onboarding content that was not touched by the mirror-sync teardown. Decide: delete, archive, or re-point to `AGENTS.md`.
- [ ] **Stale references** — prune `ensure_agent_hardlinks` / `.kilocode` mentions from `docs/plans/cleanup-plan.md` and the archive plans now that the infrastructure is gone.

## Epic: Agentic Documentation Governance

- [ ] Replace deterministic frontmatter CI gate with scoped LLM docs reviewer in CI.
- [ ] Define strict prompt contract for docs review: intent type, status, freshness, conflicts, evidence links.
- [ ] Implement non-blocking advisory mode first; collect precision/recall notes on 20+ PRs.
- [ ] Add human override label/process for contested LLM findings.
- [ ] Decide blocking threshold after advisory trial and document the promotion criteria.
- [ ] Track rollout issue and scope decisions in archived docs (see docs/archive/).
- [x] Review rules consolidated into AGENTS.md and the current execution plan.
