---
doc_type: reference
subsystem: configuration
status: active
last_reviewed: 2026-05-13
freshness: current
preservation: preserve
summary: "Reference for ColdVox compile-time feature flags and hardware configuration choices."
signals: ['features', 'configuration', 'hardware']
---
# Feature Flags & Hardware Configuration

ColdVox uses a combination of compile-time feature flags and runtime configuration to adapt to different hardware environments.

**Note:** Text injection is now a **mandatory core feature** and always enabled. It automatically compiles the correct backends for your platform.

## Quick Hardware Recommendations

| Hardware | Recommended Features | Runtime Config |
|----------|----------------------|----------------|
| **NVIDIA GPU** (Linux/Windows) | `parakeet` | `stt.preferred = "parakeet"` |
| **No GPU** (Laptop/Server) | `moonshine` | `stt.preferred = "moonshine"` |
| **AMD/Intel GPU** (Windows) | `moonshine` | `stt.preferred = "moonshine"` |
| **Universal Binary** | `parakeet`, `moonshine` | Set `preferred` based on machine |

## Feature Flags

### Core
- **`default`**: Enables `silero` (VAD). Text injection is always enabled.
- **`silero`**: Voice Activity Detection engine (enabled by default).
- **`tui`**: Terminal UI dashboard for debugging and monitoring.

### STT Backends
- **`parakeet`**: **NVIDIA-only**. High-performance STT using ONNX Runtime with CUDA/TensorRT execution providers.
- **`moonshine`**: **CPU-optimized**. Fast STT using PyO3 bindings to HuggingFace Transformers. Approx 5x faster than Whisper on CPU.

## Detailed Hardware Scenarios

### 1. Laptop (Linux, No GPU)
*   **Constraint:** CPU-only inference.
*   **Recommendation:** `moonshine`
    *   **Why:** Explicitly optimized for CPU inference.
    *   **Build:** `cargo build --release --features "moonshine"`

### 2. Laptop (Windows, Discrete GPU)
*   **Scenario A: NVIDIA GPU** -> Use `parakeet`
*   **Scenario B: AMD/Intel GPU** -> Use `moonshine` (Parakeet currently lacks DirectML support)

### 3. Desktops (Linux/Windows, NVIDIA GPUs)
*   **Constraint:** Maximum performance.
*   **Recommendation:** `parakeet`
    *   **Why:** Native CUDA/TensorRT execution provides lowest latency.
    *   **Build:** `cargo build --release --features "parakeet"`

## Runtime Configuration

Even with features enabled, you can control which backend is used via `Settings.toml` or environment variables.

```toml
[stt]
preferred = "parakeet" # or "moonshine"
fallbacks = ["moonshine"]
```

Or via env vars:
```bash
export COLDVOX__STT__PREFERRED=parakeet
```
