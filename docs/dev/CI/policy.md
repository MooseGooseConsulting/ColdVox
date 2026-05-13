---
doc_type: policy
subsystem: ci
status: active
last_reviewed: 2026-05-13
freshness: current
preservation: preserve
summary: "CI environment policy for hosted and self-hosted ColdVox runners."
signals: ['ci', 'policy', 'runners']
---
# CI Environment

Canonical CI policy is `docs/dev/CI/architecture.md`.

## Principle

- GitHub-hosted runners handle fast general CI work.
- Self-hosted Fedora/Nobara runner handles hardware-dependent tests.

## Do not use

- Xvfb on self-hosted runner
- `apt-get` on Fedora runner
- `DISPLAY=:99` in self-hosted jobs
