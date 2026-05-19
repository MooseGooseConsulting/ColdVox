---
doc_type: architecture
subsystem: general
status: active
freshness: current
preservation: reference
summary: Branching strategy, automerge policy, and self-hosted vs GitHub-hosted CI split
last_reviewed: 2026-05-19
owners: Documentation Working Group
version: 2.0.0
---

# CI Architecture

---

## Branching Strategy — Two Trunk Branches

> **ColdVox has two long-lived trunk branches. Not one. Two.**

| Branch | Purpose | PR target for |
|--------|---------|---------------|
| **`main`** | Stable trunk. CLI-only codebase, proven features. | Bug fixes, new crates, CLI features, dependency bumps |
| **`tauri-base`** | GUI integration trunk. Tauri v2 + React overlay shell. | GUI work, frontend tooling, Tauri-specific wiring |

Both branches have **branch protection rules** and **required status checks**. Both are permanent. Neither is a feature branch.

### Why two trunks?

The Tauri GUI integration is a large, ongoing effort that touches build tooling, dependencies, and project structure in ways that would destabilize `main`. Rather than a long-lived feature branch that drifts, `tauri-base` is a first-class trunk with its own CI gates and merge policy. Work flows **one direction**: `main` → `tauri-base` (periodic merges to keep GUI work current), never the reverse until the GUI is production-ready.

### Automerge (tauri-base only)

PRs into `tauri-base` use fully autonomous AI-gated automerge. No human in the loop.

**The two gates:**

| Gate | Purpose | What catches |
|------|---------|-------------|
| **CI checks** (5 required) | Mechanical correctness | Compilation failures, test regressions, lint violations |
| **AI reviewer** (CodeRabbit) | Semantic correctness | Nonsensical code, architectural violations, logic errors |

CI alone is not sufficient — an AI agent can produce code that compiles and passes tests but is wrong. The AI review is the quality gate. Both must pass.

**The pipeline:**

1. AI agent opens a PR targeting `tauri-base`
2. `.github/workflows/automerge.yml` enables `gh pr merge --auto` on the PR
3. CodeRabbit reviews the diff (`request_changes_workflow: true`, `profile: assertive`)
4. CI checks run in parallel
5. If CodeRabbit approves AND CI passes → GitHub auto-merges the PR
6. If CodeRabbit requests changes → PR blocks until issues are resolved

CodeRabbit's configuration is **global** (managed in the CodeRabbit web dashboard, not repo-level YAML). There is no `.coderabbit.yaml` in the repo.

**`main` has no automerge.** All merges to `main` are manual and must originate from `tauri-base`. This is enforced by `.github/workflows/gate-main.yml`, which fails any PR to `main` whose source branch is not `tauri-base`.

### Rules for agents and contributors

- **Always check which trunk you're targeting.** A PR that touches GUI code goes to `tauri-base`. Everything else goes to `main`.
- **Never merge `tauri-base` back into `main`** unless explicitly instructed. The reverse merge (`main` → `tauri-base`) is routine.
- **Both branches run CI.** Don't assume one is "less important" — both have required checks that must pass.

---

## Runner Architecture

> **Principle**: GitHub-hosted runners only do cheap deterministic gates; heavy
> Rust, container, and live/hardware coverage runs on self-hosted capacity or by
> explicit manual dispatch.

## Overview

ColdVox CI splits workloads between GitHub-hosted and self-hosted runners using two questions:

1. **Is this cheap enough to spend hosted minutes on every PR?**
2. **Does this task require local hardware, containers, or a warm Rust build cache?**

| Requires Laptop? | Task | Runner |
|------------------|------|--------|
| No | Repo integrity, `cargo fmt --check`, docs validation, telemetry schema | GitHub-hosted, path-filtered |
| No, but heavyweight | Workspace `cargo check`, `cargo build`, `cargo clippy`, `cargo doc`, `cargo test --workspace` | Self-hosted Fedora/Nobara or manual/nightly full CI |
| **Yes** | Hardware/live tests (display, audio, clipboard) and container-backed Parakeet integration | Self-hosted, manual `workflow_dispatch` unless explicitly scheduled |

### Windows CI (Planned)

ColdVox targets Windows via Tauri GUI. Linux-only CI is insufficient — Windows compilation, platform-specific behavior, and GUI integration must be tested on a real Windows runner.

| Runner | Purpose | Minute cost |
|--------|---------|-------------|
| GitHub-hosted Linux (`ubuntu-latest`) | Repo integrity, rustfmt, docs, telemetry schema, branch/label gates | 1x, path-filtered and concurrency-cancelled |
| Self-hosted Linux (Nobara laptop) | Cargo build/test/clippy/doc, hardware tests, container/live validation | 0 (free) |
| **Self-hosted Windows (planned)** | **Windows build, Tauri GUI tests, platform checks** | **0 (free)** |

**Status**: Self-hosted Windows runner setup is pending. See TODO in project tracking.

---

## Why Split?

### 1. Hardware Isolation

The self-hosted runner is a laptop with **weak hardware but a live display**. GitHub-hosted runners have **powerful hardware but billable minutes**.

- **Laptop/self-hosted**: Runs heavyweight Rust compilation/tests plus any real display/audio/clipboard/container coverage.
- **GitHub-hosted**: Runs only cheap deterministic gates such as repo integrity, rustfmt, docs validation, telemetry schema checks, and branch/label automation.

### 2. Parallelism

GitHub-hosted jobs run in parallel on separate machines. Self-hosted queues on one laptop.

```
Push PR:
  GitHub:      [repo integrity] [rustfmt] [docs/telemetry if path-matched]
  Self-hosted: [cargo check/build] [clippy+doc] [unit tests]
  Manual:      [live text injection] [Whisper golden master] [Parakeet container]

Hosted spend stays bounded by the cheap path-filtered jobs; heavyweight jobs queue on free self-hosted capacity.
```

### 3. No Wasted Work

Docs-only changes do not start the Rust workspace build/test pipeline. Feature-branch pushes do not start hosted CI; opening or updating a PR into `main` or `tauri-base` does.

---

## Self-Hosted Runner Environment

### Critical Facts

| Fact | Implication |
|------|-------------|
| **Live KDE Plasma 6.5.3 session** | No Xvfb needed. Use real `$DISPLAY`. |
| **Fedora/Nobara Linux** | `apt-get` does not exist. Use `dnf`. |
| **Always available** | Auto-login configured, survives reboots. |
| **Warm sccache** | Incremental builds are fast (~2-3 min). |
| **Real hardware** | Display, audio capture, clipboard all work. |

### What Breaks CI

| Mistake | Why It Breaks |
|---------|---------------|
| `GabrielBB/xvfb-action` | Internally calls `apt-get` (doesn't exist) |
| `sudo apt-get install` | Wrong package manager |
| `DISPLAY=:99` | Conflicts with real display (`:0`) |
| Moving heavyweight builds/tests back to hosted runners | Burns billable minutes on every PR |
| Running live/hardware tests on hosted runners | Hosted runners lack the required display/audio/clipboard devices |

### Current Burn-Down Guardrails

Wave 1 burn-down keeps default PR CI cheap and avoids stale Linux display setup:

- Workflow concurrency groups are workflow-specific (`ci-full-*`, `ci-minimal-*`, `docs-ci-*`) so one workflow does not cancel another on the same ref.
- `ci-minimal.yml` is the default branch/PR gate for `main` and `tauri-base`; feature-branch pushes do not spend hosted minutes, but PRs into either trunk still validate.
- Docs-only PRs route to `docs-ci.yml`; Rust workspace builds/tests are path-filtered out unless code/config/CI inputs changed.
- `ci.yml` is full/nightly validation only (`workflow_dispatch` or schedule). It is not a broad push/PR trigger.
- Default PR CI does not hydrate Whisper models or install Faster-Whisper. Whisper golden-master coverage is quarantined to nightly/manual live-runner paths.
- Self-hosted Fedora/Nobara jobs must use the live desktop session provided by the runner. They must not start Xvfb or force `DISPLAY=:99`.
- Expensive AI/docs review workflows remain advisory/shadow mode. Default hosted docs CI uploads the deterministic semantic packet and skips external LLM calls unless a human/agent explicitly runs them outside the default gate.

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                      GITHUB-HOSTED (ubuntu-latest)              │
│             Cheap, path-filtered, billable-minute work          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐   │
│  │ repo checks  │ │ rustfmt      │ │ docs / telemetry     │   │
│  │ mechanical   │ │ fmt --check  │ │ path-matched only    │   │
│  │ no full test │ │ no build     │ │ no LLM by default    │   │
│  └──────────────┘ └──────────────┘ └──────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
              ║                            
              ║  (parallel, no waiting)    
              ║                            
┌─────────────────────────────────────────────────────────────────┐
│                 SELF-HOSTED (Fedora/Nobara)                     │
│         Free minutes, warm Rust cache, live desktop             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────┐ ┌──────────────────┐ ┌───────────────┐  │
│  │ cargo check/build│ │ clippy/doc/tests │ │ manual live    │  │
│  │ default PR gate  │ │ default PR gate  │ │ Whisper/Parakeet│  │
│  └──────────────────┘ └──────────────────┘ └───────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                     ┌─────────────────┐
                     │   ci-success    │
                     │   (aggregate)   │
                     └─────────────────┘
```

---

## Speed Optimizations (Self-Hosted)

### 1. sccache (Compiler Cache)
```bash
SCCACHE_CACHE_SIZE="20G"
sccache --start-server
export RUSTC_WRAPPER=$(which sccache)
```

### 2. mold Linker (3-5x Faster Linking)
```bash
# Install once:
sudo dnf install mold

# In CI:
RUSTFLAGS="-C link-arg=-fuse-ld=mold"
```

### 3. Incremental Compilation
```bash
CARGO_INCREMENTAL="1"  # Default, but explicit
```

### 4. Targeted Build
```bash
# Instead of full workspace:
cargo build -p coldvox-text-injection -p coldvox-app
```

### 5. No Dependency Waiting
```yaml
hardware:
  runs-on: [self-hosted, Linux, X64, Fedora, Nobara]
  # NO 'needs:' clause - starts immediately
```

---

## Proposed ci.yml

```yaml
name: CI

on:
  push:
    branches: [main, release/*, feature/*, feat/*, fix/*]
  pull_request:
    branches: [main]

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  # ═══════════════════════════════════════════════════════════════
  # GITHUB-HOSTED: Fast parallel checks, NO BUILD
  # ═══════════════════════════════════════════════════════════════

  lint:
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --locked -- -D warnings

  security:
    runs-on: ubuntu-latest
    timeout-minutes: 5
    continue-on-error: true  # Advisory, don't block
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-audit cargo-deny --locked || true
      - run: cargo audit || true
      - run: cargo deny check

  # ═══════════════════════════════════════════════════════════════
  # SELF-HOSTED: Hardware/display-only tests (runs immediately, no waiting)
  # ═══════════════════════════════════════════════════════════════

  hardware:
    runs-on: [self-hosted, Linux, X64, Fedora, Nobara]
    # NO 'needs:' - starts in parallel with GitHub-hosted jobs
    timeout-minutes: 15
    env:
      CARGO_INCREMENTAL: "1"
      SCCACHE_CACHE_SIZE: "20G"
      RUSTFLAGS: "-C link-arg=-fuse-ld=mold"
      RUST_LOG: info
    steps:
      - uses: actions/checkout@v4

      - name: Start sccache
        run: |
          sccache --start-server 2>/dev/null || true
          echo "RUSTC_WRAPPER=$(which sccache)" >> "$GITHUB_ENV"

      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: "nobara-hardware"
          cache-on-failure: true

      - name: Validate display environment
        run: |
          echo "DISPLAY=$DISPLAY"
          echo "WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
          xset -q >/dev/null 2>&1 || { echo "::error::No X display"; exit 1; }
          echo "✓ Display accessible"

      - name: Hardware integration tests
        run: |
          cargo test -p coldvox-text-injection \
            --features real-injection-tests \
            --locked \
            -- --nocapture --test-threads=1

      - name: Hardware capability checks
        env:
          COLDVOX_E2E_REAL_INJECTION: "1"
          COLDVOX_E2E_REAL_AUDIO: "1"
        run: |
          cargo test -p coldvox-app --test hardware_check \
            --locked -- --nocapture --include-ignored

      - name: sccache stats
        if: always()
        run: sccache --show-stats || true

  # ═══════════════════════════════════════════════════════════════
  # AGGREGATOR
  # ═══════════════════════════════════════════════════════════════

  ci-success:
    runs-on: ubuntu-latest
    needs: [lint, hardware]
    if: always()
    steps:
      - name: Check results
        run: |
          echo "## CI Results" >> $GITHUB_STEP_SUMMARY
          echo "| Job | Result |" >> $GITHUB_STEP_SUMMARY
          echo "|-----|--------|" >> $GITHUB_STEP_SUMMARY
          echo "| lint | ${{ needs.lint.result }} |" >> $GITHUB_STEP_SUMMARY
          echo "| hardware | ${{ needs.hardware.result }} |" >> $GITHUB_STEP_SUMMARY

          if [[ "${{ needs.lint.result }}" != "success" ]]; then
            echo "::error::Lint failed"
            exit 1
          fi
          if [[ "${{ needs.hardware.result }}" != "success" ]]; then
            echo "::error::Hardware tests failed"
            exit 1
          fi
          echo "✅ CI passed"
```

---

## Common Mistakes to Avoid

### DON'T: Use Xvfb on self-hosted
```yaml
# WRONG - runner has live display
- uses: GabrielBB/xvfb-action@v1  # Also uses apt-get internally
```

### DON'T: Use apt-get
```yaml
# WRONG - this is Fedora, not Ubuntu
- run: sudo apt-get install -y xdotool
```

### DON'T: Hardcode DISPLAY=:99
```yaml
# WRONG - real display is :0
env:
  DISPLAY: ":99"
```

### DON'T: Make self-hosted wait
```yaml
# WRONG - adds 5-10 min delay
hardware:
  needs: [lint, build]
```

### DON'T: Build on GitHub-hosted
```yaml
# WRONG - wasted work, can't share with Fedora
- run: cargo build --workspace  # On ubuntu-latest
```

---

## History

| Date | Change | Reason |
|------|--------|--------|
| 2026-04-11 | Add two-trunk branching, AI-gated automerge, gate-main, Windows CI (planned) | Autonomous AI merge pipeline for tauri-base |
| 2025-12-24 | Remove Xvfb, add mold, remove waiting | PR #310 broke CI with apt-get on Fedora |
| 2025-09-19 | Initial self-hosted runner setup | Enable hardware testing |

---

## References

- [Self-hosted runner setup](../../tasks/ci-runner-readiness-proposal.md) (outdated - references Xvfb)
- PR #310: Introduced broken Xvfb infrastructure
- PR #276: Jules draft that caused the issue
