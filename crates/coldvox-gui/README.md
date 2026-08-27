# ColdVox GUI

ColdVox uses a **Tauri v2 + React** overlay shell under this folder.

The Tauri backend wires all commands and events that the React frontend expects:

- collapsed idle presence
- expanded transcript panel
- visible state feedback (`idle`, `listening`, `processing`, `ready`, `error`)
- clear separation between live partial text and committed final text
- typed Tauri command/event seam exercised by a built-in demo driver
- seam for real STT/capture wiring via `update_partial_transcript`,
  `update_final_transcript`, `set_overlay_listening`, `set_overlay_processing`,
  and `stop_overlay_capture` commands

It does **not** wire real STT, injection, hotkeys, or settings persistence yet.

## Layout

```text
crates/coldvox-gui/
├── src/                    # React frontend
│   ├── components/
│   ├── contracts/
│   ├── hooks/
│   └── lib/
└── src-tauri/              # Rust Tauri host shell package
    └── src/
        ├── lib.rs          # Tauri app bootstrap + all command handlers
        ├── contract.rs     # Shared snapshot/event types (Rust ↔ TS contract)
        ├── state.rs        # OverlayModel state machine
        ├── demo.rs         # Demo script and DemoStep type
        └── window.rs       # Window sizing helpers
```

## Key Entry Points

- Frontend shell: [`src/App.tsx`](./src/App.tsx)
- Frontend contract hook: [`src/hooks/useOverlayShell.ts`](./src/hooks/useOverlayShell.ts)
- Rust command handlers: [`src-tauri/src/lib.rs`](./src-tauri/src/lib.rs)
- Rust state model: [`src-tauri/src/state.rs`](./src-tauri/src/state.rs)

## Development Commands

Run these from `crates/coldvox-gui/`:

```bash
npm install
npm run test
npm run build
npm run tauri dev
```

Rust verification still happens through the workspace package:

```bash
cargo check -p coldvox-gui
cargo test -p coldvox-gui
```

## Wiring Real STT

When real STT arrives, the pipeline can drive the overlay by calling the
following Tauri commands (already registered and backed by `OverlayModel`):

| Command | Purpose |
|---|---|
| `set_overlay_listening` | New utterance started |
| `update_partial_transcript` | Stream partial words |
| `set_overlay_processing` | Finalising utterance |
| `update_final_transcript` | Committed result |
| `stop_overlay_capture` | End of capture session |

## Current Runtime Reality

- The frontend renders a restrained overlay shell.
- The Rust side owns window sizing, bootstrap, all command handlers, and
  event emission.
- The demo driver (`start_pipeline` command) exercises the full state machine
  without touching the real audio/STT runtime.
