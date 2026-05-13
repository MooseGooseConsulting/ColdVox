---
doc_type: plan
subsystem: stt
status: proposed
created: 2026-03-25
author: claude
summary: Add HTTP remote STT plugin to ColdVox for OpenAI-compatible transcription services
signals: ['stt', 'plugin', 'http', 'openai-api', 'streaming']
---

# Plan: HTTP Remote STT Plugin

## Problem

ColdVox has two in-process STT backends (Moonshine via PyO3, Parakeet via ONNX Runtime). Both require
compile-time dependencies and have platform-specific issues (PyO3 needs Python, ONNX GPU broken on SM120).

We now have **5 benchmarked STT models** running as local HTTP servers on this machine, all serving
OpenAI-compatible `/v1/audio/transcriptions` endpoints. ColdVox has no way to call them.

## Solution

Add a single HTTP remote plugin that implements `SttPlugin` and can talk to **any** OpenAI-compatible
STT endpoint. One plugin, all models, ~200 lines of Rust, ~3ms overhead on 150ms+ inference.

## Phase 1: Batch HTTP Plugin (this plan)

### Architecture

```
Audio frames → SttProcessor → PluginAdapter → HttpRemotePlugin
                                                    │
                                           process_audio(): buffer samples
                                           finalize(): POST WAV to endpoint
                                                    │
                                           ┌────────┴────────┐
                                           │  HTTP POST       │
                                           │  multipart/form  │
                                           │  WAV file        │
                                           └────────┬────────┘
                                                    │
                                           Parse {"text": "..."} response
                                                    │
                                           TranscriptionEvent::Final
```

### Benchmarked Services (all tested March 25, 2026, RTX 5090)

| Service | Port | Endpoint | Latency (4s clip) | VRAM | Notes |
|---|---|---|---|---|---|
| Moonshine base | 5096 | `/v1/audio/transcriptions` | **309ms** | 0 GB | CPU, always-on |
| Moonshine tiny | 5096 | `/v1/audio/transcriptions` | **158ms** | 0 GB | CPU, fastest coexist |
| Parakeet-TDT-0.6B v2 | 8200 | `/audio/transcriptions` | **86ms** | ~4 GB | GPU Docker |
| IBM Granite 4.0 1B | 5093 | `/v1/audio/transcriptions` | **780ms** | 4.3 GB | GPU, #1 accuracy |
| Qwen3-ASR-1.7B | 5094 | `/v1/audio/transcriptions` | **1.07s** | 14-24 GB | GPU, 52 languages |
| Voxtral-Mini-4B | 5095 | `/v1/audio/transcriptions` | **~4.9s** | 8.25 GB | GPU, slow on Windows |

Full benchmark data: `docs/reference/stt-docker-containers.md`

### Default target: Moonshine base on port 5096

- 0 VRAM (CPU ONNX) — coexists with any GPU workload
- 309ms for 4s clip — good enough for dictation
- Already installed at `D:\LocalLargeLanguageModels\stt-eval\moonshine\`
- Start: `cd stt-eval/moonshine && ./venv/Scripts/python.exe server.py`

---

## Implementation

### Step 1: Add dependencies to `crates/coldvox-stt/Cargo.toml`

```toml
# Under [dependencies]
reqwest = { version = "0.12", features = ["multipart", "json"], optional = true }
hound = { version = "3.5", optional = true }  # already present for moonshine

# Under [features]
http-remote = ["dep:reqwest", "dep:hound"]
```

`reqwest` is the standard async HTTP client for Rust. `hound` encodes PCM samples to WAV bytes
for the multipart upload. `hound` is already a dependency (used by moonshine), so it just needs
to be available under the new feature flag too.

### Step 2: Create `crates/coldvox-stt/src/plugins/http_remote.rs`

This is the core implementation. ~200-300 lines.

```rust
//! HTTP Remote STT Plugin
//!
//! Sends audio to any OpenAI-compatible `/v1/audio/transcriptions` endpoint.
//! Buffers PCM frames during speech, encodes to WAV on finalize, POSTs to service.

use crate::plugin::{PluginCapabilities, PluginInfo, SttPlugin, SttPluginFactory};
use crate::types::{TranscriptionConfig, TranscriptionEvent};
use async_trait::async_trait;
use std::path::Path;

/// Configuration for the HTTP remote plugin.
#[derive(Debug, Clone)]
pub struct HttpRemoteConfig {
    /// Base URL of the STT service (e.g., "http://localhost:5096")
    pub base_url: String,
    /// API path (e.g., "/v1/audio/transcriptions" or "/audio/transcriptions")
    pub api_path: String,
    /// Model name to send in the request (e.g., "moonshine/base", "granite")
    pub model_name: String,
    /// Display name for logging/UI
    pub display_name: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Sample rate (must match what the service expects, typically 16000)
    pub sample_rate: u32,
}

impl Default for HttpRemoteConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:5096".into(),
            api_path: "/v1/audio/transcriptions".into(),
            model_name: "moonshine/base".into(),
            display_name: "Moonshine (HTTP)".into(),
            timeout_ms: 10_000,
            sample_rate: 16_000,
        }
    }
}

#[derive(Debug)]
pub struct HttpRemotePlugin {
    config: HttpRemoteConfig,
    client: reqwest::Client,
    audio_buffer: Vec<i16>,
    utterance_id: u64,
    initialized: bool,
}

// Key methods to implement on SttPlugin:

// info() → PluginInfo {
//     id: "http-remote",
//     name: config.display_name,
//     requires_network: true,  // <-- important distinction
//     is_local: true,          // localhost, not cloud
//     is_available: check health endpoint
// }

// capabilities() → PluginCapabilities {
//     streaming: false,        // Phase 1 is batch only
//     batch: true,
//     word_timestamps: false,  // depends on service
//     auto_punctuation: true,  // most services do this
//     ...
// }

// is_available() → HEAD/GET to base_url/health or base_url/v1/models
//   If 200 → true, else false
//   This enables automatic failover in plugin_manager.rs

// process_audio(samples: &[i16]) → None
//   Just append to audio_buffer. No network call.
//   The SttProcessor only calls this during active speech (after VAD).

// finalize() → POST multipart form to endpoint
//   1. Encode audio_buffer to WAV bytes (hound::WavWriter to Vec<u8>)
//   2. Build multipart form:
//        - file: WAV bytes (filename "audio.wav", mime "audio/wav")
//        - model: config.model_name
//        - response_format: "json"
//   3. POST to {base_url}{api_path}
//   4. Parse response JSON: {"text": "transcribed text"}
//   5. Return TranscriptionEvent::Final { utterance_id, text, words: None }
//   6. Clear audio_buffer

// reset() → clear audio_buffer, increment utterance_id

// load_model() → no-op (model is managed by the service)

// unload() → no-op
```

#### WAV encoding helper

```rust
fn encode_wav(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>, ColdVoxError> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(&mut cursor, spec)
        .map_err(|e| ColdVoxError::SttError(format!("WAV encode: {e}")))?;
    for &s in samples {
        writer.write_sample(s)
            .map_err(|e| ColdVoxError::SttError(format!("WAV write: {e}")))?;
    }
    writer.finalize()
        .map_err(|e| ColdVoxError::SttError(format!("WAV finalize: {e}")))?;
    Ok(cursor.into_inner())
}
```

#### Health check for availability detection

```rust
async fn is_available(&self) -> Result<bool, ColdVoxError> {
    let health_url = format!("{}/health", self.config.base_url);
    match self.client.get(&health_url)
        .timeout(std::time::Duration::from_secs(2))
        .send().await
    {
        Ok(resp) if resp.status().is_success() => Ok(true),
        _ => {
            // Fallback: try /v1/models
            let models_url = format!("{}/v1/models", self.config.base_url);
            match self.client.get(&models_url)
                .timeout(std::time::Duration::from_secs(2))
                .send().await
            {
                Ok(resp) if resp.status().is_success() => Ok(true),
                _ => Ok(false),
            }
        }
    }
}
```

### Step 3: Create `HttpRemotePluginFactory`

```rust
#[derive(Debug)]
pub struct HttpRemotePluginFactory {
    config: HttpRemoteConfig,
}

impl HttpRemotePluginFactory {
    pub fn new(config: HttpRemoteConfig) -> Self {
        Self { config }
    }

    /// Convenience constructors for known services
    pub fn moonshine_base() -> Self {
        Self::new(HttpRemoteConfig::default())
    }

    pub fn moonshine_tiny() -> Self {
        Self::new(HttpRemoteConfig {
            model_name: "moonshine/tiny".into(),
            display_name: "Moonshine Tiny (HTTP)".into(),
            ..Default::default()
        })
    }

    pub fn granite() -> Self {
        Self::new(HttpRemoteConfig {
            base_url: "http://localhost:5093".into(),
            model_name: "granite".into(),
            display_name: "IBM Granite 4.0 1B (HTTP)".into(),
            ..Default::default()
        })
    }

    pub fn parakeet_gpu() -> Self {
        Self::new(HttpRemoteConfig {
            base_url: "http://localhost:8200".into(),
            api_path: "/audio/transcriptions".into(), // note: no /v1/ prefix
            model_name: "parakeet".into(),
            display_name: "Parakeet TDT 0.6B v2 (Docker)".into(),
            ..Default::default()
        })
    }

    pub fn qwen3_asr() -> Self {
        Self::new(HttpRemoteConfig {
            base_url: "http://localhost:5094".into(),
            model_name: "qwen3-asr".into(),
            display_name: "Qwen3-ASR-1.7B (HTTP)".into(),
            ..Default::default()
        })
    }
}

#[async_trait]
impl SttPluginFactory for HttpRemotePluginFactory {
    fn plugin_id(&self) -> &str { "http-remote" }
    fn display_name(&self) -> &str { &self.config.display_name }

    async fn check_requirements(&self) -> Result<bool, ColdVoxError> {
        // Just check if the service is reachable
        let client = reqwest::Client::new();
        let url = format!("{}/health", self.config.base_url);
        Ok(client.get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false))
    }

    async fn create(&self) -> Result<Box<dyn SttPlugin>, ColdVoxError> {
        Ok(Box::new(HttpRemotePlugin::new(self.config.clone())))
    }
}
```

### Step 4: Register in `crates/coldvox-stt/src/plugins/mod.rs`

Add:
```rust
#[cfg(feature = "http-remote")]
pub mod http_remote;
#[cfg(feature = "http-remote")]
pub use http_remote::{HttpRemotePlugin, HttpRemotePluginFactory, HttpRemoteConfig};
```

### Step 5: Wire into `crates/app/Cargo.toml`

```toml
[features]
# Existing
moonshine = ["coldvox-stt/moonshine"]
parakeet = ["coldvox-stt/parakeet"]
# New
http-remote = ["coldvox-stt/http-remote"]
```

### Step 6: Register factory in `crates/app/src/stt/plugin_manager.rs`

In the `load_config()` or registry initialization, add:

```rust
#[cfg(feature = "http-remote")]
{
    use coldvox_stt::plugins::http_remote::HttpRemotePluginFactory;

    // Register default (Moonshine base)
    registry.register(Box::new(HttpRemotePluginFactory::moonshine_base()));

    // Optionally register others based on config
    // registry.register(Box::new(HttpRemotePluginFactory::granite()));
    // registry.register(Box::new(HttpRemotePluginFactory::parakeet_gpu()));
}
```

### Step 7: Configuration via `config/plugins.json`

Add HTTP remote configuration to the existing config schema:

```json
{
  "preferred_plugin": "http-remote",
  "http_remote": {
    "base_url": "http://localhost:5096",
    "api_path": "/v1/audio/transcriptions",
    "model_name": "moonshine/base",
    "timeout_ms": 10000
  },
  "fallbacks": ["moonshine", "mock"]
}
```

This lets the user switch services without recompiling.

### Step 8: Tests

Add `crates/coldvox-stt/src/plugins/http_remote_tests.rs`:

1. **Unit test**: `encode_wav` produces valid WAV bytes (decode with hound, verify samples match)
2. **Unit test**: `process_audio` buffers correctly, `reset` clears buffer
3. **Integration test**: Mock HTTP server (wiremock or similar) → verify POST format, parse response
4. **Integration test**: Real service test (gated behind `#[ignore]` or env var) against Moonshine

---

## Phase 2: Streaming (future, separate plan)

### What streaming means here

Phase 1 sends the entire utterance after VAD detects speech-end. Phase 2 would send audio chunks
**during speech** and receive partial transcripts in real-time.

### Protocol options

| Protocol | Latency | Complexity | Services |
|---|---|---|---|
| WebSocket | Low | Medium | Custom servers, some ASR APIs |
| gRPC (Riva) | Lowest | High | NVIDIA NIM Nemotron Streaming |
| Server-Sent Events | Medium | Low | Some custom servers |
| Chunked HTTP | Medium | Low | Voxtral Realtime (vLLM) |

### Streaming architecture sketch

```
Audio frames → SttProcessor → PluginAdapter → HttpStreamingPlugin
                                                    │
                                           process_audio(): send chunk via WebSocket
                                                    │ (partial transcript back)
                                           TranscriptionEvent::Partial { text }
                                                    │
                                           finalize(): close stream
                                                    │
                                           TranscriptionEvent::Final { text, words }
```

### Streaming candidates

1. **NVIDIA Nemotron Streaming 0.6B** — gRPC/Riva, <24ms latency, not yet tested on SM120
2. **Moonshine** — Has Python streaming API, would need WebSocket wrapper
3. **Voxtral Realtime** — Designed for streaming, needs vLLM on Linux

### Prerequisites for Phase 2

- Phase 1 plugin working and tested
- Decide on WebSocket vs gRPC (recommend WebSocket for simplicity)
- Build/find a streaming-capable STT server (wrap Moonshine or Nemotron)
- Extend `SttPlugin::process_audio()` to return `Some(TranscriptionEvent::Partial)` during speech

### Effort estimate

- Phase 2 WebSocket streaming: ~500-800 lines (plugin + server wrapper)
- Phase 2 gRPC/Riva streaming: ~1000+ lines (protobuf codegen, tonic client)

---

## File Manifest

| File | Action | Lines (est) |
|---|---|---|
| `crates/coldvox-stt/Cargo.toml` | Edit: add reqwest, feature flag | ~5 |
| `crates/coldvox-stt/src/plugins/http_remote.rs` | **Create** | ~250 |
| `crates/coldvox-stt/src/plugins/mod.rs` | Edit: add module + re-export | ~4 |
| `crates/app/Cargo.toml` | Edit: add http-remote feature | ~2 |
| `crates/app/src/stt/plugin_manager.rs` | Edit: register factory | ~10 |
| `crates/coldvox-stt/src/plugins/http_remote_tests.rs` | **Create** | ~150 |
| **Total** | | **~420 lines** |

## Risks

| Risk | Mitigation |
|---|---|
| STT service not running → plugin unavailable | `is_available()` health check + failover to mock/moonshine |
| Network timeout on slow models (Voxtral 5s) | Configurable `timeout_ms`, default 10s |
| WAV encoding overhead | Negligible: ~1ms for 30s of audio. hound is fast. |
| reqwest adds compile time | Feature-gated, only compiled when `http-remote` enabled |
| Service returns unexpected JSON | Defensive parsing with clear error messages |

## Success Criteria

- [ ] `cargo build --features http-remote` compiles cleanly
- [ ] Plugin registers and appears in plugin list
- [ ] `is_available()` returns true when Moonshine server is running, false when not
- [ ] Transcribing ColdVox test WAVs returns correct text
- [ ] Failover works: if HTTP service dies, falls back to configured fallback plugin
- [ ] Config-driven: changing `base_url` switches between Moonshine/Granite/Parakeet without recompile
- [ ] All existing tests still pass (no regressions)
