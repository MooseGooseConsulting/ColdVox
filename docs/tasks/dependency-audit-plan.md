---
doc_type: task-plan
subsystem: dependencies
status: active
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Methodology for conducting a rigorous dependency audit."
signals: ['dependencies', 'audit', 'task-plan']
---
# Rigorous Dependency Audit Methodology

**Objective:** Execute an exhaustive, package-by-package engineering audit of every direct and critical transitive dependency in the ColdVox project. This is not an automated scan; it is a manual, architectural investigation into the version delta, compatibility matrix, and integration health of our dependencies across Rust, Python, and System levels.

## Phase 1: Exhaustive Version Mapping & Delta Analysis
For **every single** direct dependency in `Cargo.toml` and `pyproject.toml/uv.lock`:
1.  **Version State:** Document the exact currently locked version vs. the latest stable release available upstream.
2.  **Changelog Investigation:** Read the release notes and changelogs for every major and minor version between our locked version and the latest.
3.  **Architectural Shift Identification:** Explicitly identify any fundamental changes in the dependency's architecture. Examples:
    *   Transitioning from synchronous to asynchronous APIs.
    *   Deprecation of key traits or interfaces we currently rely on.
    *   Changes in their underlying C/C++ bindings or system requirements.
    *   Shifts in memory management or thread safety guarantees.

## Phase 2: Cross-Ecosystem Compatibility Verification
ColdVox spans Rust and Python. An upgrade in one ecosystem can fatally break the other.
1.  **The PyO3 Boundary:** For any Rust crate exposed to Python via PyO3 (e.g., `coldvox-stt`), audit the PyO3 version compatibility matrix against our target Python versions (currently managed by `uv`). If we upgrade PyO3, what happens to the ABI?
2.  **C-ABI & FFI Fragility:** For dependencies wrapping C libraries (like `rubato` or anything touching audio/CUDA), verify if upstream version bumps require a newer system-level shared library (`.dll`/`.so`). 
3.  **Hardware Alignment (Blackwell/CUDA 12.8+):** Specifically for `torch`, `torchaudio`, and STT backend dependencies (Moonshine/Parakeet): Investigate if newer versions introduce regressions or optimizations for SM 12.0 (RTX 5090). We cannot upgrade blindly if a new version breaks Blackwell compatibility.

## Phase 3: Dependency Conflict & Matrix Resolution
Before proposing an upgrade, prove it won't break the rest of the tree.
1.  **Transitive Collision:** If we upgrade Crate A, will it force a major version bump in a shared transitive dependency (like `tokio` or `serde`) that Crate B cannot support?
2.  **Python Native Constraints:** Verify that the native wheels provided by the latest Python packages (`torch` etc.) are compiled against the exact CUDA version we intend to enforce on the host system.

## Phase 4: Obsolescence and Eviction
Dependencies are liabilities.
1.  **Dead/Abandoned Projects:** Identify any dependency that has not had a commit in 12 months. Propose a concrete architectural plan to replace or rewrite its functionality.
2.  **Feature Flag Minimization:** Audit the exact features enabled for every Rust crate. Strip every feature flag that is not demonstrably required by our codebase to reduce the compilation surface area.

## Execution Output Requirements
The executing agent must not return a simple list of outdated packages. The final deliverable (`docs/research/exhaustive-dependency-audit-[date].md`) MUST contain:
*   A package-by-package breakdown of the version delta.
*   A detailed summary of **Architectural Breaking Changes** for every proposed upgrade.
*   A **Compatibility Risk Assessment** for how an upgrade impacts the PyO3 boundary or hardware execution (CUDA).
*   A strict, step-by-step **Upgrade Sequencing Plan** to avoid dependency hell.