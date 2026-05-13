---
doc_type: plan
subsystem: architecture
status: archived
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Archived proposal for a unified ColdVox architecture."
signals: ['architecture', 'archive', 'proposal']
---
# Option C: Unified ColdVox Architecture

> **Status**: PROPOSAL (2026-03-24)
> **Goal**: Merge the best of ColdVox (Rust) and ColdVox_Mini (Python/Tauri) into a single, high-performance Windows dictation app optimized for NVIDIA 5090 / CUDA.

## Context

ColdVox and ColdVox_Mini each solve half the problem well:

| Component | ColdVox (Rust) | ColdVox_Mini (Python) | Winner |
|---|---|---|---|
| Audio capture | `coldvox-audio` (cpal) | Python sounddevice | **ColdVox** |
| VAD | `coldvox-vad-silero` (pure Rust ONNX) | Silero via Python ONNX | **ColdVox** |
| STT engine | `parakeet-rs` (CUDA/TensorRT via ort-rs) | `whisper.dll` (Vulkan ctypes) | **ColdVox** (5090) |
| Text injection | ydotool/AT-SPI (Linux) | Hybrid keyboard sim + clipboard (Windows) | **Mini** |
| GUI | Qt/QML TUI (incomplete) | Tauri v2 + React "Dynamic Island" (polished) | **Mini** |
| Voice commands | None | "new line", "scratch that", punctuation | **Mini** |
| Platform | Linux (Wayland/X11) | Windows 10/11 | **Mini** |

## Architecture

```
┌──────────────────────────────────────────────┐
│            Tauri v2 Shell (from Mini)         │
│  ┌────────────────────────────────────────┐   │
│  │  React Frontend (Dynamic Island UI)    │   │
│  │  - Glassmorphism floating pill         │   │
│  │  - State: Idle → Listening → Done      │   │
│  │  - Voice command feedback              │   │
│  └──────────────┬─────────────────────────┘   │
│                 │ tauri::command invoke()      │
│  ┌──────────────▼─────────────────────────┐   │
│  │  Rust Backend (from ColdVox crates)    │   │
│  │                                        │   │
│  │  coldvox-audio ──► coldvox-vad-silero  │   │
│  │       │                    │            │   │
│  │       ▼                    ▼            │   │
│  │  coldvox-stt (parakeet-cuda)           │   │
│  │       │                                │   │
│  │       ▼                                │   │
│  │  text-injection (Windows native)       │   │
│  └────────────────────────────────────────┘   │
└──────────────────────────────────────────────┘
```

## What We Take From Each

### From ColdVox (Rust crates)
- `coldvox-audio` — microphone capture via cpal (already cross-platform)
- `coldvox-vad-silero` — pure Rust Silero VAD
- `coldvox-stt` with `parakeet-cuda` feature — NVIDIA Parakeet 1.1B via ort-rs
  - ✅ Verified: `cargo check -p coldvox-stt --features parakeet,parakeet-cuda` passes cleanly
- `coldvox-foundation` — error types, shared primitives
- `coldvox-telemetry` — tracing/metrics infrastructure

### From ColdVox_Mini (Python/Tauri)
- `gui/` — Tauri v2 + React + TypeScript + Framer Motion frontend
- Voice command processor logic (port from Python to Rust)
- Windows text injection strategy (hybrid keyboard sim + clipboard)
- PTT hotkey handling via `GetAsyncKeyState` (port to Rust `windows` crate)
- `config.yaml` schema and user-facing configuration

## What We Drop

- **Moonshine STT** — replaced by Parakeet (no more PyO3 bridge fragility)
- **whisper.dll / Vulkan path** — replaced by parakeet-rs CUDA
- **Linux-specific injection** (ydotool, AT-SPI) — Windows-only target for now
- **Qt/QML GUI** — replaced by Tauri/React from Mini
- **All Python runtime** — fully Rust backend, zero Python dependency

## Implementation Phases

### Phase 1: Validate Parakeet Runtime (GATE)
Before committing to the merge, verify `parakeet-rs` actually transcribes on the 5090:
1. Build `coldvox-stt` with `parakeet-cuda`
2. Run the `verify_moonshine` example adapted for parakeet
3. Confirm model download, CUDA session creation, and transcription output

### Phase 2: Scaffold Tauri App
1. Replace `crates/coldvox-gui/` with a Tauri v2 app
2. Copy Mini's React frontend into `crates/coldvox-gui/src/`
3. Wire `tauri::command` handlers to call `coldvox-audio`, `coldvox-vad-silero`, `coldvox-stt`

### Phase 3: Windows Text Injection
1. Port Mini's hybrid injection to Rust using `windows` crate or `enigo`
2. Integrate with the existing `coldvox-text-injection` crate interface

### Phase 4: Voice Commands & Polish
1. Port Mini's command processor from Python to Rust
2. End-to-end testing: mic → VAD → STT → injection on Windows

## Critical Implementation Details ("The Dragons")

### 1. Global Hotkeys (PTT)
Unlike the TUI, a background dictation app Needs to listen to keys even when blurred.
- **Solution**: Use the `rdev` crate for cross-platform global listeners, or directly use `GetAsyncKeyState` via the `windows` crate on a background thread.
- **Logic**: 
  - On KeyDown(Hotkey) -> Start Capture
  - On KeyUp(Hotkey) -> Stop Capture & Start Inference

### 2. Voice Command Processor
We need to port the logic from Mini's `voice_commands.py` to a Rust module in `coldvox-stt` or a new `coldvox-commands` crate.
- **Features**: 
  - Token matching (e.g., "new line" -> `\n`)
  - Semantic actions (e.g., "scratch that" -> Backspace last word)
  - Auto-punctuation (if not handled by Parakeet)

### 3. Packaging & Model Distribution (Heavyweight)
The final binary will be massive if bundled, or brittle if separate.
- **Payload**:
  - `onnxruntime.dll` (CUDA/TensorRT) ~ 200MB
  - Parakeet 1.1B Model ~ 1.1GB
  - Silero VAD ~ 2MB
- **Strategy**: Use Tauri's "Sidecar" or a custom `setup` hook to download models to `%LOCALAPPDATA%` on first run if not present.

---

## Path to Production: Packaging & Deployment

### 4. Installation & Runtime Dependencies
This is the single biggest "user friction" point.
- **Problem**: CUDA apps usually require the user to install a 3GB NVIDIA driver/toolkit.
- **Solution**: We must bundle the specific `onnxruntime_providers_cuda.dll` and its dependencies (or use a static build of ORT if possible).
- **Installer**: Use Tauri's **WiX** or **NSIS** bundler to create a standard Windows `.msi` or `.exe`.
- **Requirement Check**: The app should check for a compatible NVIDIA GPU on startup and fallback gracefully to a "Non-compatible hardware" screen if no CUDA is found.

### 5. Auto-Updates
Since the model (~1GB) and the binary (~50MB) are separate, the update strategy matters.
- **Core Update**: Use Tauri's built-in updater for the `.exe` and UI.
- **Model Update**: The Rust backend should check a `version.txt` on the model CDN and re-download the Parakeet ONNX file if it changes, rather than re-bundling it in every app update.

### 6. Final UX Polish
- **Dynamic Island States**: 
  - `IDLE`: Small pill, grey icon.
  - `LISTENING`: Expanding pill, pulsing red icon/waveform.
  - `THINKING`: Spinning loader (Inference).
  - `INJECTING`: Success checkmark.
- **Settings GUI**: A secondary window for:
  - Hotkey rebinding.
  - Custom voice commands (User-defined shortcuts).
  - Model selection (Small vs Large Parakeet).

---

## Pain Points & Help Needed Matrix

| Feature | Difficulty | Transition Logic | Help Needed |
|---|---|---|---|
| **CUDA Runtime** | 🔴 High | Moving from "it works on my machine" to "it works on every 5090". | DLL sideloading expert. |
| **Global Hooks** | 🟡 Med | Ensuring low-latency hotkey detection without being flagged by Anti-Cheat. | Windows Low-Level Hooking. |
| **Command Port** | 🟢 Low | Rewriting Python string logic to Rust. | Just "boring" work. |
| **Installer** | 🟡 Med | Handling the 1GB+ payload without crashing common installers. | WiX / CI Pipeline tuning. |

---

## Build Location: ColdVox Repo

We will build inside the existing **ColdVox** repository:
1. **Pros**: Direct access to verified `parakeet-rs` crates, existing workspace structure, and GitHub Actions.
2. **Action**: `rm -rf crates/coldvox-gui` (the legacy Qt/QML stub) and replace it with the Tauri v2 scaffold.

---

## Next Step: Phase 1 (Validation)
Before scaffolding the GUI, we MUST prove the engine works.
- **Task**: Run `cargo run -p coldvox-stt --example verify_parakeet --features parakeet,parakeet-cuda` (from crate directory, experimental)
- **Success Criteria**: Real audio input results in correct text on the terminal.
