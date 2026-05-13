---
doc_type: standard
subsystem: agent-workflow
status: active
last_reviewed: 2026-05-13
freshness: current
preservation: preserve
summary: "Working rules for agent activity in the ColdVox repository."
signals: ['agents', 'standards', 'workflow']
---
# Working Rules

## DO

- Use `cargo {cmd} -p {crate}` for iteration speed, but finish with `cargo check --workspace --all-targets`.
- Only use live testing (real microphone/`.wav` files) to test VAD and STT. Do not mock audio buffers.
- Check `docs/plans/current-status.md` for what currently works and what's broken.

## DO NOT

- Claim Whisper or Parakeet are currently production-ready.
- Modify Python dependencies without using `uv`.
- Auto-run commands that destroy data or commit unverified changes.
