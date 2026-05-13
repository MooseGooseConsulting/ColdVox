---
doc_type: session
subsystem: windows
status: complete
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Session log for Windows stability and pipeline recovery work on 2026-03-25."
signals: ['windows', 'session', 'stability', 'recovery']
---
# Session Log: Windows Stability & Pipeline Recovery (2026-03-25)

## Objective
Establish a stable, verified baseline for ColdVox on Windows 11, moving from "Unit Tests Only" to "Live Hardware Verified".

## Timeline & Actions

### 1. Documentation Triage (The Cleanup)
- **Problem**: 20+ stale or contradictory plan files were creating "hallucination loops" for AI agents.
- **Action**: Archived 7 critical plans, deleted 10 obsolete meta-docs, and consolidated current guidance around the active recovery plan and anchor docs.
- **Result**: Cognitive load reduced; project health is now transparent.

### 2. Live Hardware Verification (Audio Stage)
- **Problem**: Unsure if WASAPI actually worked in this restricted environment.
- **Action**: Ran `coldvox-audio` live hardware tests with the `--features live-hardware-tests` gate.
- **Result**: **SUCCESS**.
    - WASAPI detected 3 microphones (including USB default).
    - Captured 288,000 samples in 3 seconds at 48kHz stereo.
    - Signal flow verified from driver -> CPAL -> RingBuffer -> Chunker.

### 3. STT Pipeline Recovery (The Moonshine Patch)
- **Problem**: `coldvox-app` failed to compile with the `moonshine` feature enabled.
- **Action**: 
    - Patched `crates/app/src/runtime.rs` to fix `Instant` usage.
    - Corrected `stop_gc_task` and `stop_metrics_task` return type mismatches (API drift).
- **Result**: `coldvox-app` now compiles cleanly with full STT support.

### 4. The "Security Policy" Battle (OS Error 4551)
- **Blocker**: Windows Application Control (Smart App Control/WDAC) blocks unsigned build artifacts.
- **The Struggle**:
    - **Step A**: Binary `coldvox.exe` was blocked. User "fixed it" (likely a specific file exemption).
    - **Step B**: `pyo3-build-config` build script was blocked. Identified it was in `target/debug/build`.
    - **Step C**: `STATUS_DLL_NOT_FOUND`. The binary couldn't find `python312.dll`. Solved by force-copying the DLL from the `uv` cache into `target/debug`.
    - **Step D**: `ModuleNotFoundError`. Python couldn't find its internal `encodings` module. Plan: Set `PYTHONHOME`.
    - **Step E**: **Back to 4551**. Every time `cargo test` runs, it generates a **new binary hash** (e.g., `golden_master-321433ba925001b4.exe`), which triggers a fresh block.

## Current State
- **Audio Capture**: ✅ VERIFIED (Real samples captured).
- **VAD**: ✅ VERIFIED (Silero ONNX functional).
- **STT**: ⚠️ BLOCKED by "Whack-a-Mole" security hashes. 
- **Injection**: ✅ VERIFIED (Enigo unit tests pass).

## Recommendation for Stability
To stop the "Whack-a-Mole" hash game, the **entire project folder** and **Cargo registry** must be exempted as **Trusted Locations**, not just individual files.

1. `D:\_projects\ColdVox\target\` (Recursive)
2. `C:\Users\pmacl\.cargo\registry\` (Recursive)
