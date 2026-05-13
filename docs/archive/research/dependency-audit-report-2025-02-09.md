---
doc_type: research
subsystem: dependencies
status: archived
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Archived dependency audit report from 2025-02-09."
signals: ['dependencies', 'audit', 'archive']
---
# Dependency Audit Report - 2025-02-09

## Summary of Findings

The ColdVox workspace dependency audit reveals a multi-crate Rust project with extensive Python integration via PyO3 for the moonshine STT backend. The Rust workspace comprises 10 crates with no major duplicated dependencies identified in the tree analysis. Python dependencies are managed exclusively via `uv`, with a clean environment containing 69 packages, primarily for audio processing (torch, librosa), machine learning (transformers, tokenizers), and testing (pytest). Cross-language bindings involve numerous native extensions (.pyd files on Windows), with PyO3 enabling Rust-Python interoperability in the STT crate. System dependencies include CUDA 12.8 for GPU acceleration, with proper PATH configuration. Security audit tools (cargo-audit, deny) were not available locally, but no obvious vulnerabilities were noted in dependency trees. The deprecated `tool.uv.dev-dependencies` field in pyproject.toml should be updated to `dependency-groups.dev`.

## Vulnerability & Obsolescence List

- **Security Tools Unavailable:** Local installation of `cargo-audit` and `cargo-deny` failed due to system policy restrictions. Recommend running these in CI or on a different environment.
- **Python Deprecation Warning:** `tool.uv.dev-dependencies` is deprecated; migrate to `dependency-groups.dev` in pyproject.toml.
- **Outdated Package Check:** `cargo-outdated` not installed; manual checks suggest no severely outdated crates, but automated checks recommended.
- **No Known Vulnerabilities:** Based on dependency tree inspection, no prominent security issues identified. Full audit with tools advised.

## Dependency Conflicts

- **Rust Workspace:** No duplicated versions of major crates detected. Some shared dependencies like `proc-macro2` appear multiple times but are compatible versions.
- **Python Environment:** All packages resolved without conflicts. uv.lock synchronizes correctly with pyproject.toml.
- **Cross-Language:** No conflicts between Rust and Python dependencies noted.

## Native Linkage Report

- **Python Native Extensions:** 69 .pyd files identified in .venv, including critical ones like `torch\_C.cp312-win_amd64.pyd`, `tokenizers.pyd`, and numerous scipy/numpy extensions.
- **Rust Shared Libraries:** PyO3 in coldvox-stt crate produces Rust-based .dll/.pyd for Python integration. Build and inspect after compilation.
- **DLL Resolution Status:** CUDA libraries properly in PATH. Visual C++ Redistributables assumed present (version 14.x required for PyTorch). For DLL_NOT_FOUND issues:
  - Use Process Monitor (ProcMon) to log file system access during import failures.
  - Check Python interpreter version mismatches (Python 3.12 in use).
  - Ensure no conflicting python3.dll in PATH.

## Actionable Remediation Plan

1. **Install Security Audit Tools:** In CI environment, install `cargo-audit` and run `cargo audit` on workspace.
2. **Update pyproject.toml:** Change `tool.uv.dev-dependencies` to `dependency-groups.dev` and run `uv sync`.
3. **Install cargo-outdated:** Attempt installation on allowed systems and check for outdated Rust crates; update as needed with `cargo update`.
4. **Build and Inspect Rust DLLs:** Compile coldvox-stt with moonshine feature and use dumpbin or Dependency Walker to map linked libraries.
5. **Verify System Dependencies:** Confirm Visual C++ Redistributable 2015-2022 (x64) is installed for native extensions.
6. **CUDA Validation:** Test GPU execution with torch to ensure cuDNN compatibility (not explicitly checked).
7. **PATH Integrity:** No issues found, but monitor for wrong DLL loading in production.