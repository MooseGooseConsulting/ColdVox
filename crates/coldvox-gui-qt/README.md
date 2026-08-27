# ColdVox GUI — Qt 6 + QML backend

Alternative host shell for the ColdVox overlay, implemented with
**Qt 6 + QML** via the [`cxx-qt`](https://github.com/KDAB/cxx-qt) crate.

This crate is intentionally **excluded from the root Cargo workspace** so that
`cargo check --workspace` does not fail on machines without Qt installed.

---

## When to use this backend

| | Tauri backend (`coldvox-gui`) | Qt backend (this crate) |
|---|---|---|
| Runtime | WebView2 (ships with Windows 11) | Qt 6 runtime |
| UI language | TypeScript + React | QML |
| Renderer | Chromium/WebKit | Qt Scene Graph (OpenGL/Vulkan/D3D12) |
| IPC model | `invoke` + `emit` over JSON bridge | Direct property bindings + Qt signals |
| Overhead | Medium (separate JS engine) | Low (native GPU scene graph) |
| Transparency | Via Tauri `transparent: true` | Via `Qt.FramelessWindowHint` |

Use the Qt backend if you need lower IPC latency for high-frequency audio
visualisers or if you prefer native Qt tooling.

---

## Build requirements

- **Qt 6.2+** (Qt Quick + Qt QML modules)
- **CMake 3.24+**
- **Rust stable** (same toolchain as the workspace)
- A C++ compiler: MSVC on Windows, GCC/Clang on Linux/macOS

### Windows (recommended)

```powershell
# Install Qt 6 via the online installer (https://www.qt.io/download-open-source)
# or via vcpkg:
vcpkg install qt6-base qt6-declarative

# Set Qt install prefix so CMake can find it
$env:CMAKE_PREFIX_PATH = "C:\Qt\6.8.0\msvc2022_64"

cargo build --manifest-path crates/coldvox-gui-qt/Cargo.toml
cargo run  --manifest-path crates/coldvox-gui-qt/Cargo.toml
```

### Linux

```bash
# Fedora / RHEL
sudo dnf install qt6-qtbase-devel qt6-qtdeclarative-devel

# Debian / Ubuntu
sudo apt install qt6-base-dev qt6-declarative-dev

cargo build --manifest-path crates/coldvox-gui-qt/Cargo.toml
cargo run  --manifest-path crates/coldvox-gui-qt/Cargo.toml
```

---

## Architecture

```text
crates/coldvox-gui-qt/
├── Cargo.toml            # Standalone package (NOT in root workspace)
├── build.rs              # cxx-qt-build — generates C++/CMake glue
├── src/
│   ├── main.rs           # QGuiApplication + QQmlApplicationEngine entry
│   ├── overlay_bridge.rs # cxx-qt QObject: properties, signals, invokables
│   └── demo.rs           # Demo script steps (mirrors Tauri backend)
└── qml/
    └── Overlay.qml       # Full overlay window: collapsed + expanded states
```

### Bridge design

The `OverlayBridge` QObject is declared using the `#[cxx_qt::bridge]` macro
and registered as a QML element (`OverlayBridge { id: overlay }`).

**Properties** (auto-notified):

| Property | Type | Description |
|---|---|---|
| `status` | `string` | `idle \| listening \| processing \| ready \| error` |
| `statusDetail` | `string` | Human-readable state description |
| `partialTranscript` | `string` | Live provisional text |
| `finalTranscript` | `string` | Committed result |
| `expanded` | `bool` | Collapsed/expanded window state |
| `paused` | `bool` | Pause flag |
| `errorMessage` | `string` | Most recent error (empty = none) |

**Signals**:

| Signal | Description |
|---|---|
| `transcriptReady()` | Emitted when status reaches `ready` |
| `errorRaised()` | Emitted when an invalid command is rejected |

**Invokables (QML-callable)**:

| Method | Description |
|---|---|
| `start_pipeline()` | Start the overlay demo |
| `stop_pipeline()` | Stop demo/capture |
| `toggle_pause()` | Pause/resume |
| `clear_transcript()` | Reset all transcript state |
| `set_expanded(bool)` | Collapse/expand |
| `open_settings()` | Settings placeholder |
| `demo_tick()` | Advance demo by one step (called by QML Timer) |
| `apply_partial_transcript(str)` | Feed live partial from STT |
| `apply_final_transcript(str)` | Commit final result from STT |
| `set_processing()` | Signal STT is finalising |
| `set_listening()` | Signal new utterance started |
| `stop_capture()` | End real capture session |

### Demo driver

The demo is driven entirely by a QML `Timer` that calls `demo_tick()` every
350 ms.  This keeps the Qt event loop as the sole source of concurrency and
avoids thread-safety complexity.  The `demo_generation` counter in
`OverlayBridgeRust` lets `demo_tick()` ignore stale ticks after a stop/clear.

### Wiring real STT

When the real STT pipeline lands, connect it to:

1. `set_listening()` — new utterance started
2. `apply_partial_transcript(text)` — stream partials
3. `set_processing()` — utterance being finalised
4. `apply_final_transcript(text)` — commit result
5. `stop_capture()` — session ended

These are identical to the Tauri backend's command surface so that the
`coldvox-app` crate can target either backend with minimal changes.
