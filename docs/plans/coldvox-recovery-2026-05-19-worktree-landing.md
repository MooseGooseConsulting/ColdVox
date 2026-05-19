schema_version: 1
report_id: coldvox-recovery-2026-05-19-worktree-landing
plan_id: coldvox_recovery_2026_05_19
task_id: wave1_repo_landing
generated_at: "2026-05-19"
generated_from_branch: codex/recovery-2026-05-19

root_trunk_facts:
  remote:
    name: origin
    url: https://github.com/Coldaine/ColdVox
    default_branch: origin/main
  trunks:
    stable: origin/main
    integration: origin/tauri-base
    policy_source: docs/dev/CI/architecture.md
  landing_branch:
    name: codex/recovery-2026-05-19
    base: origin/tauri-base
    head_at_capture: "0b4935a"
    ahead_of_base_at_capture: 13
    behind_base_at_capture: 0
  guardrails_observed:
    pushed: false
    force_pushed: false
    deleted_branches: false
    merged_to_main: false
    discarded_uncommitted_work: false

worktrees_captured:
  - path: D:/_projects/ColdVox
    branch: codex/recovery-2026-05-19
    head: "0b4935aead0da06de8ff5489578ff25dbe3900b3"
    status_at_capture: clean
  - path: D:/_projects/.trees/coldvox-final-corrections
    branch: codex/final-windows-corrections
    head: "8820aba52b209723d7afbcbbac92e052d25bc5de"
    upstream: origin/codex/final-windows-corrections
    upstream_status: gone
    status_at_capture: clean
  - path: D:/_projects/.trees/coldvox-http-parakeet-recovery
    branch: feat/parakeet-http-remote-live-tests
    head: "7c0ab1083e80412acb1aabf16d8911c3ffd9d118"
    upstream: origin/feat/parakeet-http-remote-live-tests
    upstream_status: gone
    status_at_capture: clean
  - path: D:/_projects/.trees/coldvox-land-all-work
    branch: codex/post-merge-parakeet-corrections
    head: "82e50c7f437a851b9b8e9f8bc67da676cf022008"
    upstream: origin/codex/post-merge-parakeet-corrections
    upstream_status: gone
    status_at_capture: clean
  - path: D:/_projects/.trees/coldvox-test-os-scoping
    branch: codex/windows-test-os-scoping
    head: "cbeab939f753de0c4198108f9faa5e84d04db454"
    upstream: origin/codex/windows-test-os-scoping
    upstream_status: present
    status_at_capture: clean
  - path: D:/_projects/.trees/coldvox-windows-rampage
    branch: codex/restore-windows-e2e-validation
    head: "ac0e2565c43a8db7773dc0897f0464d7f11aa181"
    upstream: null
    upstream_status: no_upstream
    status_at_capture: clean

dirty_worktree_preservation:
  - worktree: D:/_projects/ColdVox
    branch: codex/recovery-2026-05-19
    method: commits
    commits:
      - sha: "a145f04"
        message: "fix(stt): use runtime http remote profile"
      - sha: "fe689f6"
        message: "docs(windows): clarify parakeet http validation path"
      - sha: "1c4aa6a"
        message: "ci: route expensive checks off hosted defaults"
      - sha: "9c61b85"
        message: "chore(parakeet): add local http container helper"
      - sha: "18bed55"
        message: "chore(parakeet): improve health check guidance"
      - sha: "0b4935a"
        message: "chore(parakeet): harden just helper invocation"
  - worktree: D:/_projects/.trees/coldvox-windows-rampage
    branch: codex/restore-windows-e2e-validation
    method: commit
    commits:
      - sha: "ac0e256"
        message: "test(windows): strengthen live validation evidence"

consolidation_results:
  merged:
    - source: codex/restore-windows-e2e-validation
      target: codex/recovery-2026-05-19
      merge_commit: "5da3a2e"
      conflicts_resolved:
        - docs/dependencies.md
        - docs/domains/foundation/fdn-testing-guide.md
    - source: feature/stt-parakeet-http-integration
      target: codex/recovery-2026-05-19
      merge_commit: "3e5bd49"
      conflicts_resolved:
        - docs/plans/parakeet-http-remote-integration-spec.md
  already_represented:
    - branch: feat/parakeet-http-remote-live-tests
      reason: cherry_unique_vs_recovery=0
    - branch: codex/final-windows-corrections
      reason: cherry_unique_vs_recovery=0
    - branch: codex/post-merge-parakeet-corrections
      reason: cherry_unique_vs_recovery=0
    - branch: codex/land-parakeet-and-agents
      reason: cherry_unique_vs_recovery=0
    - branch: codex/windows-test-os-scoping
      reason: cherry_unique_vs_recovery=0
  blocked:
    - branch: codex/recover-http-parakeet
      merge_probe: "git merge --no-ff --no-commit codex/recover-http-parakeet"
      probe_result: aborted_after_conflicts
      cherry_unique_vs_recovery: 4
      conflict_files:
        - config/windows-parakeet.toml
        - crates/app/src/main.rs
        - docs/index.md
        - docs/windows-live-runbook.md
        - scripts/run-coldvox.ps1
        - scripts/windows_live_validate.ps1
      notes:
        - AGENTS.md would auto-merge, but probe was aborted with no changes kept.
    - branch: feature/http-stt-plugin
      merge_probe: "git merge --no-ff --no-commit feature/http-stt-plugin"
      probe_result: aborted_after_conflicts
      cherry_unique_vs_recovery: 1
      conflict_files:
        - config/plugins.json
        - crates/app/config/plugins.json
        - crates/app/src/audio/vad_adapter.rs
        - crates/coldvox-stt/src/plugins/http_remote.rs
        - docs/sessions/2026-03-25-windows-stability-recovery.md
      unsafe_artifacts_that_would_be_added:
        - .omc/project-memory.json
        - .omc/state/agent-replay-c353c8a2-3c89-4125-9024-bfb4bff397ca.jsonl
        - .omc/state/hud-state.json
        - .omc/state/hud-stdin-cache.json
        - .omc/state/idle-notif-cooldown.json
        - .omc/state/last-tool-error.json
        - .omc/state/mission-state.json
        - .omc/state/sessions/c353c8a2-3c89-4125-9024-bfb4bff397ca/cancel-signal-state.json
        - .omc/state/subagent-tracking.json
        - compile_errors.txt
        - pr_all_15.txt
        - prs.txt
        - prs_jules.txt

upstream_problem_branches_at_capture:
  no_upstream:
    - chore/gitignore-editor-artifacts
    - chore/test-build-cleanup
    - codex/restore-windows-e2e-validation
    - feat/tauri-pipeline-wiring
    - feature/http-stt-plugin
    - feature/stt-parakeet-http-integration
    - pr-366
    - test-pr-365
  upstream_gone:
    - agent/coldvox/issue-triage-batch-1
    - codex/final-windows-corrections
    - codex/land-parakeet-and-agents
    - codex/post-merge-parakeet-corrections
    - codex/recover-http-parakeet
    - codex/windows-pr1-runtime
    - codex/windows-pr2-live-tests
    - codex/windows-pr3-runbook
    - dependabot/cargo/rust-dependencies-737b7b0d5c
    - feat/parakeet-http-remote-live-tests
    - feature/gui-tauri-v2-migration
    - feature/nuclear-pruning-and-stt-gc
  divergent:
    - branch: codex/recovery-2026-05-19
      upstream: origin/tauri-base
      ahead: 13
      behind: 0
    - branch: feat/always-on-push-to-transcribe
      upstream: origin/feat/always-on-push-to-transcribe
      ahead: 13
      behind: 40
    - branch: main
      upstream: origin/main
      ahead: 1
      behind: 1
    - branch: tauri-base
      upstream: origin/tauri-base
      ahead: 0
      behind: 1

validation:
  passed:
    - command: "cargo test -p coldvox-app --features http-remote test_runtime_selection_config_does_not_rewrite_persisted_config --locked"
      result: "1 passed; 0 failed"
    - command: "cargo test -p coldvox-app --features http-remote start_uses_runtime_http_remote_config_without_rewriting_plugin_persistence --locked"
      result: "1 passed; 0 failed"
    - command: "PowerShell parser check: scripts/windows_live_validate.ps1"
      result: passed
    - command: "PowerShell parser check: scripts/parakeet_http.ps1"
      result: passed
    - command: "just --summary"
      result: passed
    - command: "git diff --check"
      result: passed
    - command: "git status --short --branch"
      result: "root clean before report write; all captured worktrees clean"
  not_run:
    - just windows-run-preflight
    - just windows-smoke
    - just windows-live
    - just test
