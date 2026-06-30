# coldvox-gui-qt

Qt/CXX-Qt GUI backend for ColdVox. This is the **Qt backend** of the dual-GUI
plan — it lives alongside the Tauri backend (`crates/coldvox-gui`) and talks to
the same `coldvox_app::runtime` pipeline seam. Pick whichever GUI you want to
build; the Rust pipeline crates are shared.

## Why a Qt backend?

The Tauri backend uses WebKitGTK on Linux, which has known overlay/click-through
edge cases on Wayland. Qt's native window flags (`Qt.WindowStaysOnTopHint`,
`Qt.FramelessWindowHint`, transparent color) and `Qt.labs.settings` work
reliably across X11 and Wayland, and on KDE Plasma (which is Qt-based) the
overlay, tray, and DPI handling are first-class. On Windows the Tauri backend
is the lighter option. This crate gives you the Qt story for Linux-first
deployments.

## Build

The Qt UI is gated behind the `qt-ui` cargo feature so the default workspace
build stays stub-only and does not require Qt6 on CI runners.

```bash
# Default (stub):
cargo build -p coldvox-gui-qt

# Real Qt UI — requires Qt6 dev packages:
cargo run -p coldvox-gui-qt --features qt-ui
```

### Qt6 prerequisites

| Distro | Command |
|---|---|
| Ubuntu/Debian | `sudo apt install qt6-base-dev qt6-declarative-dev libqt6opengl6-dev qt6-qt5compat-dev` |
| Fedora | `sudo dnf install qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qt5compat-devel` |
| Arch | `sudo pacman -S qt6-base qt6-declarative qt6-5compat` |
| Windows | Qt installer or vcpkg |

(`Qt5Compat.GraphicalEffects`, used by the overlay's drop shadow, requires the
`qt6-qt5compat` package.)

## Architecture

- `src/bridge.rs` — CXX-Qt bridge. Defines `GuiBridge` (a QObject with
  `expanded`/`state`/`last_error`/`partial_transcript`/`final_transcript`
  qproperties, `transcript_partial`/`transcript_final` qsignals, and
  `cmd_start`/`cmd_stop`/`cmd_pause`/`cmd_resume`/`cmd_clear_error`/`cmd_clear`
  qinvokables). `cmd_start` spawns a dedicated Tokio runtime thread, calls
  `coldvox_app::runtime::start`, and forwards `TranscriptionEvent`s back into
  the qproperties via `CxxQtThread::queue`.
- `src/main.rs` — `QGuiApplication` + `QQmlApplicationEngine` loading
  `qml/Main.qml`.
- `qml/Main.qml` — always-on-top transparent overlay with collapsed/expanded
  states, drag-to-move, transcript surface, control bar, and
  `Qt.labs.settings` persistence.
- `qml/SettingsWindow.qml` — settings dialog scaffold.

## Status / follow-ups

- The bridge logic is complete and unit-tested offline (state transitions,
  pause/resume, clear). `cmd_stop` currently transitions UI state only; full
  graceful pipeline shutdown (holding the `AppHandle` Arc across the bridge) is
  a tracked follow-up — the Tauri backend already performs a real
  `shutdown().await`.
- The final QML↔bridge binding (registering `GuiBridge` as a QML context
  property or instantiating it via a cxx-qt qml module URI) is the remaining
  integration step; `Main.qml` uses `typeof bridge !== 'undefined'` guards so
  it renders standalone until then.
- Per-state tray icons and a real system tray menu (the Tauri backend already
  has a tray) are a follow-up for this crate.
