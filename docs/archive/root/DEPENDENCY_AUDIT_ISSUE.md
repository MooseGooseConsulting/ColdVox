---
doc_type: issue
subsystem: dependencies
status: archived
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Archived dependency audit action items from 2026-03-24."
signals: ['dependencies', 'audit', 'archive']
---
# [Dependency Audit] Action items from exhaustive dependency audit (2026-03-24)

## Summary
An exhaustive dependency audit was performed on the ColdVox project following the methodology in `docs/tasks/dependency-audit-plan.md`. The audit identified several action items to improve security, fix obsolescence, resolve native linkage issues, and prepare for future hardware (Blackwell/RTX 5090) compatibility.

## Action Items
- [x] Fix native Windows issue – add Visual C++ redistributable to CI (completed in `scripts/local_ci.sh`)
- [-] Upgrade Rust crates (tar, tokio, serde, clap, tracing, rustls, serde_json, thiserror, log) and replace `paste`
- [ ] Remove dead `whisper` feature flag from Cargo.toml files
- [ ] Upgrade Python packages (tokenizers, librosa, transformers, numpy, scipy, requests, urllib3)
- [ ] Re‑build PyO3 bindings for coldvox‑stt
- [ ] Run full audit again (cargo audit, cargo outdated, uv pip list)
- [ ] Document DLL requirements in docs/system/Windows-dll-requirements.md
- [ ] Update documentation in docs/plans/windows-multi-agent-recovery.md
- [ ] Test on Blackwell GPU with CUDA 12.8 (optional mid‑term step)

## Details
See the full audit report: [`exhaustive-dependency-audit-2026-03-24.md`](docs/research/exhaustive-dependency-audit-2026-03-24.md)

## Next Steps
1. Implement the remaining action items in the order listed.
2. After each step, run the full audit to verify progress.
3. Once all items are complete, close this issue.
