---
doc_type: reference
subsystem: stt
status: active
freshness: current
preservation: preserve
summary: Comprehensive STT model evaluation — benchmarked models, Docker containers, 2026 landscape, setup commands
signals: ['stt', 'docker', 'parakeet', 'whisper', 'gpu', 'rtx-5090', 'granite', 'moonshine', 'qwen3-asr', 'voxtral']
created: 2026-03-25
last_verified: 2026-03-25
---

# STT Reference — Models, Benchmarks & Docker Containers

Evaluated speech-to-text models and Docker containers on an RTX 5090 (32 GB VRAM, Blackwell SM120, CUDA 12.8, Windows 11).

All benchmarks run against ColdVox WAV files (`D:\_projects\ColdVox\crates\app\test_data\test_{1-5}.wav`).

**Canonical wave-1 backend profile for the HTTP container hardening workstream:** **Parakeet CPU on the cluster endpoint `http://192.168.30.207:5092`** (Deployment `parakeet` in namespace `apps`, coldaine-k8cluster repo, LoadBalancer `parakeet-lan`; same digest-pinned `ghcr.io/achetronic/parakeet` image). No local container startup is needed for the primary path — `config/default.toml` already points at it. Repo-owned docs, launcher defaults, and config comments should treat that OpenAI-compatible `/v1/audio/transcriptions` + `/health` profile as the one first-class backend for this workstream. The local compose profile (`ops/parakeet/docker-compose.yml` on `http://localhost:5092`) remains the offline dev fallback. Moonshine, Granite, Qwen3-ASR, and Voxtral remain deferred comparison/non-default profiles here.

**Frozen ColdVox client upload contract for wave 1:** ColdVox uploads mono 16 kHz 16-bit WAV audio to `POST /v1/audio/transcriptions`, probes `GET /health`, and expects JSON containing `text`. That is the ColdVox client contract only; it is not a claim that every backend only accepts that exact audio shape.

---

## Benchmarked Models — Real RTX 5090 Results (March 25, 2026)

Six models downloaded, installed, served, and benchmarked on this hardware. Sorted by latency.

### Summary Table

| # | Model | Latency (4s clip) | VRAM | Accuracy | RTF | Backend | Port |
|---|---|---|---|---|---|---|---|
| 1 | **Parakeet-TDT-0.6B v2** (marcpope) | **86ms** | ~4 GB | Perfect | ~47x RT | GPU, Docker | 8200 |
| 2 | **Moonshine tiny** (UsefulSensors) | **~158ms** | **0 GB** | Perfect | ~26x RT | CPU ONNX | 5096 |
| 3 | **Moonshine base** (UsefulSensors) | **~309ms** | **0 GB** | Perfect | ~14x RT | CPU ONNX | 5096 |
| 4 | **IBM Granite 4.0 1B Speech** | **780ms** | 4.3 GB | Perfect | ~5x RT | GPU, PyTorch | 5093 |
| 5 | **Qwen3-ASR-1.7B** (Alibaba) | **1.07s** | 14-24 GB | Perfect + cased/punctuated | ~4x RT | GPU, PyTorch | 5094 |
| 6 | **Voxtral-Mini-4B-Realtime** (Mistral) | **~4.9s** | 8.25 GB | Correct (date normalization) | **1.4x RT** | GPU, Transformers | 5095 |

### VRAM Coexistence with Qwen3.5-35B (27.6 GB at 65k context)

| Model | VRAM | Coexists with LLM? |
|---|---|---|
| Moonshine tiny/base | 0 GB | Always — CPU only |
| Parakeet-TDT-0.6B v2 | ~4 GB | Marginal (0.4 GB free) |
| IBM Granite 4.0 1B | 4.3 GB | No — would need to stop LLM |
| Voxtral-Mini-4B | 8.25 GB | No |
| Qwen3-ASR-1.7B | 14-24 GB | No (24 GB peak during inference) |

**Practical coexistence winners**: Moonshine (zero GPU) and Parakeet via Docker (can stop/start).

---

### 1. Parakeet-TDT-0.6B v2 — Speed Champion (archived comparison profile, not the canonical wave-1 container)

- **Source**: `marcpope/parakeet-api` Docker image (13.7 GB, model baked in)
- **Architecture**: NVIDIA NeMo Parakeet transducer, non-autoregressive
- **VRAM**: ~4 GB
- **API**: `/audio/transcriptions` (note: no `/v1/` prefix)
- **License**: Apache 2.0
- **Open ASR Rank**: #4 overall (Avg WER 6.05%)

**Benchmark results:**

| File | Duration | Transcription | Latency |
|---|---|---|---|
| test_1.wav | 4s | "On august twenty seventh, eighteen thirty seven, she writes Yeah." | 8.4s cold / **0.100s** |
| test_2.wav | ~5s | "Far from it, sire your majesty having given no directions about it..." | **0.130s** |
| test_3.wav | ~3s | "He is the one with the worst record." | **0.086s** |
| 10s recording | 10s | "Testing, testing, testing, record. Everybody loves..." | **0.126s** |

```bash
# Start
docker run -d --name parakeet-gpu -p 8200:8000 --gpus all marcpope/parakeet-api:latest

# Test
time curl -s -X POST http://localhost:8200/audio/transcriptions \
  -F "file=@D:/_projects/ColdVox/crates/app/test_data/test_1.wav"
```

**Source**: [hub.docker.com/r/marcpope/parakeet-api](https://hub.docker.com/r/marcpope/parakeet-api)

---

### 2. Moonshine tiny/base — Zero VRAM Champion

- **Source**: `pip install useful-moonshine-onnx` (Python 3.8+, works on 3.14)
- **Architecture**: Encoder-decoder, ONNX inference, CPU-only
- **Params**: tiny ~27M, base ~61M
- **VRAM**: 0 GB (pure CPU ONNX Runtime)
- **License**: MIT
- **Published WER**: 6.65% (base)
- **Location**: `D:\LocalLargeLanguageModels\stt-eval\moonshine\`

**Benchmark results (warm, 3 runs averaged):**

| Model | Run 1 | Run 2 | Run 3 | Avg | Transcription |
|---|---|---|---|---|---|
| moonshine/tiny | 0.156s | 0.174s | 0.146s | **~158ms** | "On August twenty seventh, eighteen thirty seven, she writes." |
| moonshine/base | 0.299s | 0.305s | 0.323s | **~309ms** | "On August twenty seventh, eighteen thirty seven, she writes." |

Model sizes: base ~57MB, tiny ~26MB (ONNX float). Load time: 1.5s from cache.

```bash
# Start server
cd D:/LocalLargeLanguageModels/stt-eval/moonshine
./venv/Scripts/python.exe server.py
# Serves on http://localhost:5096

# Test
curl -X POST http://localhost:5096/v1/audio/transcriptions \
  -F "file=@D:/_projects/ColdVox/crates/app/test_data/test_1.wav" \
  -F "model=moonshine/base" \
  -F "response_format=verbose_json"
```

**Why this matters**: Sub-200ms transcription with zero GPU — leaves full RTX 5090 free for LLM inference. Best option for always-on STT alongside llama-swap.

**Source**: [github.com/usefulsensors/moonshine](https://github.com/usefulsensors/moonshine)

---

### 3. IBM Granite 4.0 1B Speech — Accuracy Champion

- **Source**: `ibm-granite/granite-4.0-1b-speech` on HuggingFace
- **Architecture**: Conformer encoder + LLM decoder, speculative decoding
- **Params**: ~2B (BF16)
- **VRAM**: 4.32 GB allocated / 4.35 GB reserved
- **License**: Apache 2.0
- **Open ASR Rank**: **#1 overall** (Avg WER 5.52%, March 2026)
- **Location**: `D:\LocalLargeLanguageModels\stt-eval\granite\`

**Benchmark results (RTX 5090, GPU, warm):**

| File | Duration | Transcription | Latency |
|---|---|---|---|
| test_1.wav | 4s | "on august twenty seventh eighteen thirty seven she writes" | **0.78s** |
| test_2.wav | ~6s | "far from it sire your majesty having given no directions about it the musicians have retained it" | **1.49s** |
| test_3.wav | ~3s | "he is the one with the worst record" | **0.75s** |
| test_4.wav | ~26s | Long paragraph — near-perfect ("schoolboys" vs "school boys") | **5.46s** |
| test_5.wav | ~4s | "your play must be not merely a good play but a successful one" | **1.03s** |

Model loads in ~2s from HuggingFace cache. Cold start (first download): ~33s.

```bash
# Start server
cd D:/LocalLargeLanguageModels/stt-eval/granite
./venv/Scripts/python.exe server.py
# Serves on http://localhost:5093

# Test
time curl -s -X POST http://localhost:5093/v1/audio/transcriptions \
  -F "file=@D:/_projects/ColdVox/crates/app/test_data/test_1.wav" \
  -F "model=granite"
```

**Key insight**: Best accuracy at only 4.3 GB VRAM. The #1 ASR model in the world runs comfortably on RTX 5090. Uses `transformers` + `AutoModelForSpeechSeq2Seq`. Requires `soundfile` for audio loading (torchaudio removed in PyTorch 2.11).

**Source**: [huggingface.co/ibm-granite/granite-4.0-1b-speech](https://huggingface.co/ibm-granite/granite-4.0-1b-speech)

---

### 4. Qwen3-ASR-1.7B — Best Multilingual + Formatting

- **Source**: `pip install qwen-asr` (single package, transformers backend)
- **Architecture**: Qwen audio encoder + LLM decoder
- **Params**: 1.7B
- **VRAM**: 14.2 GB idle, **~24 GB peak** during inference
- **License**: Apache 2.0
- **Languages**: 52
- **Location**: `D:\LocalLargeLanguageModels\stt-eval\qwen3-asr\`

**Benchmark results (RTX 5090, GPU, warm):**

| File | Duration | Transcription | Latency |
|---|---|---|---|
| test_1.wav | 4s | "On August twenty seventh, eighteen thirty seven, she writes." | **1.07s** |

Cold start (CUDA JIT warmup): 11.2s. Warm latency: ~1s for 4s clip.

**Unique features**: Proper casing + punctuation in output (no post-processing needed). 52-language support. Same Qwen family as our LLM stack.

```bash
# Start server
cd D:/LocalLargeLanguageModels/stt-eval/qwen3-asr
./venv/Scripts/python.exe server.py
# Serves on http://localhost:5094

# Test
curl -X POST http://localhost:5094/v1/audio/transcriptions \
  -F "file=@D:/_projects/ColdVox/crates/app/test_data/test_1.wav" \
  -F "model=qwen3-asr"
```

**Caveat**: 24 GB peak VRAM makes it impossible to coexist with Qwen3.5-35B. `qwen-asr-serve` wraps vLLM and only exposes `/v1/chat/completions` — our `server.py` provides the Whisper-compatible `/v1/audio/transcriptions` endpoint using the transformers backend directly.

**Source**: [github.com/QwenLM/Qwen3-ASR](https://github.com/QwenLM/Qwen3-ASR)

---

### 5. Voxtral-Mini-4B-Realtime — Disappointing on Windows

- **Source**: `mistralai/Voxtral-Mini-4B-Realtime-2602` on HuggingFace
- **Architecture**: ~3.4B LM + ~970M audio encoder
- **Params**: ~4B (BF16)
- **VRAM**: 8.25 GB
- **License**: Apache 2.0
- **Location**: `D:\LocalLargeLanguageModels\stt-eval\voxtral\`

**Benchmark results (RTX 5090, GPU, Transformers backend, warm):**

| File | Duration | Transcription | Latency | RTF |
|---|---|---|---|---|
| test_1.wav | 4s | "On August 27, 1837, she writes," | 4861ms | 1.22x |
| test_2.wav | 6.5s | Correct | 9416ms | 1.44x |
| test_3.wav | 2.2s | Correct | 3488ms | 1.57x |
| test_4.wav | 26.1s | Correct (long passage) | 31838ms | 1.22x |
| test_5.wav | 4s | Correct | 6487ms | 1.64x |

**Average RTF: 1.42x — slower than realtime** under the Transformers backend.

```bash
# Start server
cd D:/LocalLargeLanguageModels/stt-eval/voxtral
./venv/Scripts/python.exe server.py
# Serves on http://localhost:5095

# Test
curl -X POST http://localhost:5095/v1/audio/transcriptions \
  -F "file=@D:/_projects/ColdVox/crates/app/test_data/test_1.wav" \
  -F "model=voxtral-mini" \
  -F "response_format=verbose_json"
```

**Why it's slow**: The Voxtral paper claims <500ms latency — but that's with vLLM 0.10+ on Linux. vLLM doesn't install on Windows natively (PyPI only has 0.8.3, needs 0.10+). The Transformers 5.3.0 path is the only working approach on Windows without WSL/Docker. The model normalizes dates/numbers (e.g., "eighteen thirty seven" → "1837") which may or may not be desirable.

**Source**: [huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)

---

## Models Not Yet Tested (from ChatGPT research, PDF dated March 25, 2026)

These models were identified in a systematic review but have not been downloaded or benchmarked yet.

### NVIDIA Nemotron Speech Streaming en-0.6B

- **Repo**: `nvidia/nemotron-speech-streaming-en-0.6b`
- **Params**: 600M
- **Updated**: March 12-13, 2026 (fresh checkpoint)
- **Self-reported WER**: LibriSpeech clean 2.32 / other 4.84; TED-LIUM 3.50
- **Key feature**: Streaming-native with configurable chunk sizes (latency/WER tradeoff), built-in punctuation + capitalization
- **License**: NVIDIA Open Model License
- **VRAM**: Should fit comfortably in 32GB
- **Why interesting**: Purpose-built for real-time streaming. Configurable chunk sizes let you trade latency for accuracy. Could be the best option for live dictation.
- **Available via**: HuggingFace, NIM container (`nemotron-asr-streaming`)

### Microsoft VibeVoice-ASR-HF

- **Repo**: `microsoft/VibeVoice-ASR-HF`
- **Params**: ~8B (BF16)
- **Released**: 2026-01-26, added to Transformers 2026-03-02
- **Open ASR WER**: Avg 7.77; LibriSpeech clean 2.20 / other 5.51; TED-LIUM 2.57
- **Key feature**: **ASR + diarization + timestamps** in a single model. Handles 60-min audio in one pass.
- **License**: MIT
- **VRAM**: Fits 32GB BF16 for moderate-length audio; long-form may cause OOM
- **RTFx**: 51x (Open ASR Leaderboard, A100)
- **Why interesting**: Only model that does transcription + speaker identification in one forward pass. Useful for meeting recordings or multi-speaker audio.

```python
from transformers import AutoProcessor, VibeVoiceAsrForConditionalGeneration
model_id = "microsoft/VibeVoice-ASR-HF"
processor = AutoProcessor.from_pretrained(model_id)
model = VibeVoiceAsrForConditionalGeneration.from_pretrained(model_id, device_map="auto")
```

### Voxtral Small 24B

- **Repo**: `mistralai/Voxtral-Small-24B-2507`
- **Params**: ~24.3B
- **VRAM**: ~55GB in bf16/fp16 — **won't fit on single RTX 5090 without 4-bit quantization**
- **WER**: LibriSpeech clean 1.53 / other 3.14 (excellent)
- **License**: Apache 2.0
- **Status**: Skip unless multi-GPU or heavy quantization is acceptable

### Whisper Large v3 (baseline)

- **Repo**: `openai/whisper-large-v3`
- **Params**: 1.55B
- **VRAM**: ~10GB
- **WER**: LibriSpeech clean 1.84 / other 3.66
- **License**: MIT
- **Status**: Battle-tested baseline. Available via faster-whisper-server Docker, NIM, or native. Outperformed by Granite 4.0 on accuracy and Parakeet on speed.

---

## Docker Containers — Tested (March 2026)

### 1. marcpope/parakeet-api (Fastest — GPU Parakeet, archived comparison profile)

- **Image**: `marcpope/parakeet-api:latest` (13.7 GB)
- **Port**: 8200 (mapped from 8000)
- **API**: `/audio/transcriptions` (no `/v1/` prefix)
- **GPU**: Yes (CUDA, NeMo + PyTorch, Parakeet-TDT 0.6B v2 baked in)
- **VRAM**: ~4 GB
- **Tested**: Yes — **86ms for 3s clip**, fastest tested
- **ColdVox plugin id**: `http-remote-parakeet-gpu` (optional comparison profile)

```bash
docker run -d --name parakeet-gpu -p 8200:8000 --gpus all marcpope/parakeet-api:latest
```

Repo-owned launcher path:

```bash
docker compose -f ops/parakeet/docker-compose.yml --profile gpu up -d parakeet-gpu
```

**Caveat**: API endpoint is `/audio/transcriptions` (with liveness on `/healthz`) rather than the canonical CPU profile’s `/v1/audio/transcriptions` + `/health`. Keep this image as an optional GPU comparison profile, not as the first-wave ColdVox container default.

**Source**: [hub.docker.com/r/marcpope/parakeet-api](https://hub.docker.com/r/marcpope/parakeet-api)

---

### 2. fedirz/faster-whisper-server (Best balance — GPU Whisper)

- **Image**: `fedirz/faster-whisper-server:latest-cuda` (7.17 GB)
- **Port**: 8100 (mapped from 8000)
- **API**: OpenAI-compatible `/v1/audio/transcriptions`
- **GPU**: Yes (CUDA, auto-downloads models)
- **VRAM**: ~2 GB (small), ~6-10 GB (large-v3)
- **Tested**: Yes

```bash
docker run -d --name faster-whisper \
  --gpus all -p 8100:8000 \
  -v faster-whisper-models:/root/.cache/huggingface \
  fedirz/faster-whisper-server:latest-cuda
```

**Test results (Whisper small):**

| File | Duration | Latency |
|---|---|---|
| test_1.wav | 4s | 0.585s |
| test_2.wav | ~5s | 0.731s |
| test_3.wav | ~3s | 0.392s |
| test_5.wav | ~4s | 0.489s |

Supports model swapping: `Systran/faster-whisper-tiny`, `small`, `medium`, `large-v3`.

**Source**: [hub.docker.com/r/fedirz/faster-whisper-server](https://hub.docker.com/r/fedirz/faster-whisper-server)

---

### 3. ghcr.io/achetronic/parakeet (CPU-only — Zero VRAM, canonical wave-1 profile)

- **Image**: `ghcr.io/achetronic/parakeet:latest` (1.31 GB)
- **Port**: 5092
- **API**: OpenAI-compatible `/v1/audio/transcriptions`
- **GPU**: No (CPU only, ONNX Runtime)
- **VRAM**: 0 GB
- **Tested**: Yes — 1.9s for 4s clip

```bash
docker run -d --name parakeet-cpu -p 5092:5092 ghcr.io/achetronic/parakeet:latest
```

**Use case**: Canonical wave-1 ColdVox HTTP container profile — OpenAI-compatible `POST /v1/audio/transcriptions` and `GET /health`, while avoiding GPU contention. The primary deployment is the k8s cluster endpoint `http://192.168.30.207:5092` (apps/parakeet in coldaine-k8cluster; no local startup needed). Running it locally via `docker run` above or `ops/parakeet/docker-compose.yml` on `http://localhost:5092` is the offline dev fallback.

**Source**: [github.com/achetronic/parakeet](https://github.com/achetronic/parakeet)

---

### 4. NVIDIA NIM Containers (Best quality — Broken on SM120)

Requires NGC API key (stored in BWS as `NGC_API_KEY`). Uses Triton inference server internally.

**Status: NOT WORKING on RTX 5090 SM120** — TensorRT compilation fails, A100 FP16 profile loads but crashes under inference. 72 GB of NIM images should be deleted.

```bash
# These NIM images are broken on RTX 5090 and should be removed:
docker rmi nvcr.io/nim/nvidia/parakeet-0-6b-ctc-en-us:rtx-latest    # 33.6 GB
docker rmi nvcr.io/nim/nvidia/parakeet-1-1b-ctc-en-us:latest         # 38.6 GB
```

Available NIM containers (for reference, not tested working):

| Container ID | Model | Streaming | Notes |
|---|---|---|---|
| `parakeet-0-6b-ctc-en-us` | CTC 0.6B | No | Has `rtx-latest` tag |
| `parakeet-1-1b-ctc-en-us` | CTC 1.1B | No | Best EN accuracy |
| `parakeet-0.6b-tdt` | TDT 0.6B | No | #1 Open ASR Leaderboard |
| `nemotron-asr-streaming` | Nemotron 0.6B | Yes | Lowest latency (<24ms) |

**Source**: [docs.nvidia.com/nim/speech/latest](https://docs.nvidia.com/nim/speech/latest/reference/support-matrix/asr.html)

---

### 5. ghcr.io/speaches-ai/speaches (STT + TTS combo)

- **Image**: `ghcr.io/speaches-ai/speaches:latest-cuda` (8.58 GB)
- **Port**: 8000
- **API**: OpenAI-compatible `/v1/audio/transcriptions` + TTS
- **Tested**: Container runs but model install has `AssertionError: CachedRepoInfo` bug.

```bash
docker run -d --name speaches --gpus all -p 8000:8000 \
  -v speaches-models:/home/ubuntu/.cache/huggingface/hub \
  ghcr.io/speaches-ai/speaches:latest-cuda
```

**Source**: [github.com/speaches-ai/speaches](https://github.com/speaches-ai/speaches)

---

### 6. mekopa/whisperx-blackwell (Diarization — Untested)

- **Image**: `mekopa/whisperx-blackwell:latest`
- **Port**: 8003
- **API**: REST `/transcribe` (not OpenAI-compatible)
- **Features**: WhisperX large-v3 + word alignment + speaker diarization
- **Caveat**: Targets datacenter Blackwell (SM121). RTX 5090 is SM120 — may not work.

**Source**: [github.com/Mekopa/whisperx-blackwell](https://github.com/Mekopa/whisperx-blackwell)

---

## Open ASR Leaderboard (March 2026)

| Rank | Model | Avg WER | Speed (RTFx) | Params | Notes |
|---|---|---|---|---|---|
| **1** | **IBM Granite 4.0 1B Speech** | **5.52%** | 280x | 2B | **Tested — 780ms, 4.3 GB** |
| 2 | NVIDIA Canary-Qwen-2.5B | 5.63% | 418x | 2.5B | |
| 3 | IBM Granite-Speech-3.3-8B | 5.85% | — | ~9B | |
| **4** | **Parakeet-TDT-0.6B v2** | **6.05%** | **3,386x** | 0.6B | **Tested — 86ms, ~4 GB** |
| 5 | Parakeet-TDT-0.6B v3 | 6.34% | ~3,333x | 0.6B | |
| 6 | Nemotron Streaming 0.6B | 6.93% | real-time | 0.6B | Not tested |
| 7 | Parakeet-CTC-1.1B | 7.40% | 2,728x | 1.1B | |
| 8 | Whisper Large v3 | 7.44% | 145x | 1.55B | |
| — | Microsoft VibeVoice-ASR | 7.77% | 51x | 9B | Not tested |
| — | Voxtral-Mini-4B (480ms config) | 8.47% | — | 4B | **Tested — 4.9s (Transformers)** |

### LibriSpeech WER (clean/noisy)

| Model | test-clean | test-other |
|---|---|---|
| Granite 4.0 1B | 1.42% | 2.85% |
| Voxtral Small 24B | 1.53% | 3.14% |
| Parakeet-TDT-0.6B v2 | 1.69% | 3.19% |
| Whisper large-v3 | 1.84% | 3.66% |
| Voxtral-Mini-4B (480ms) | 2.08% | 5.54% |
| VibeVoice-ASR | 2.20% | 5.51% |
| Nemotron Streaming (1.12s) | 2.32% | 4.84% |

**Sources**: [HuggingFace Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard), [IBM Research](https://research.ibm.com/blog/granite-speech-recognition-hugging-face-chart), ChatGPT research PDF (March 25, 2026)

---

## 2026 ASR Landscape

### Architecture Trends

1. **Conformer + LLM decoder** is the winning recipe (Canary-Qwen, Granite, VibeVoice)
2. **Scale efficiency** is the 2026 theme — Granite 4.0 achieving #1 with only 1B params
3. **No Whisper v4** — OpenAI shifted to closed API models (`gpt-4o-transcribe`)
4. **Streaming via sliding-window attention** gaining traction (Moonshine v2, Voxtral, Nemotron)
5. **CPU-only viable** — Moonshine proves sub-200ms is possible without GPU

### All Models Evaluated (Combined)

| Model | Tested? | Latency | VRAM | WER (Avg) | Best For |
|---|---|---|---|---|---|
| Parakeet-TDT-0.6B v2 | **Yes** | 86ms | ~4 GB | 6.05% | Speed |
| Moonshine tiny | **Yes** | 158ms | 0 GB | — | Zero-VRAM always-on |
| Moonshine base | **Yes** | 309ms | 0 GB | 6.65% | Zero-VRAM quality |
| IBM Granite 4.0 1B | **Yes** | 780ms | 4.3 GB | **5.52%** | Best accuracy |
| Qwen3-ASR-1.7B | **Yes** | 1.07s | 14-24 GB | — | Multilingual, formatting |
| Voxtral-Mini-4B | **Yes** | 4.9s | 8.25 GB | 8.47% | Needs vLLM (Linux) |
| Nemotron Streaming 0.6B | No | — | Low | 6.93% | Real-time streaming |
| VibeVoice-ASR 8B | No | — | ~16 GB | 7.77% | Diarization + timestamps |
| Voxtral Small 24B | No | — | ~55 GB | ~5.5% | Won't fit 32GB |
| Whisper large-v3 | Docker only | 0.5s | ~2-10 GB | 7.44% | Baseline, 99 languages |

---

## Operational Notes

### Cold Start Times

| Model/Container | First-ever start | Warm start |
|---|---|---|
| Moonshine (pip) | ~43s (model download) | **1.5s** |
| Granite 4.0 1B (pip) | ~33s (model download) | **2s** |
| Qwen3-ASR (pip) | ~108s (3.6 GB download) | **4s** |
| Voxtral-Mini (pip) | ~120s (model download) | **8s** |
| marcpope/parakeet-api (Docker) | ~5 min (model baked in) | **15-20s** |
| faster-whisper-server (Docker) | ~1 min + model download | **10-15s** |
| Parakeet CPU (Docker) | ~2 min | **5s** |

### Streaming Support

| Model | Streaming | Protocol | Notes |
|---|---|---|---|
| Nemotron Streaming 0.6B | **Yes** | gRPC (Riva) | <24ms latency, not tested |
| Moonshine | Partial | Python API | Built-in streaming in `moonshine-voice` package |
| All others tested | No | HTTP batch | Send file → get text |

### API Compatibility

All tested models serve OpenAI-compatible `/v1/audio/transcriptions` except:
- **marcpope/parakeet-api**: `/audio/transcriptions` (no `/v1/` prefix)
- **whisperx-blackwell**: `/transcribe` (custom REST)

Standard test command:
```bash
curl -X POST http://localhost:<port>/v1/audio/transcriptions \
  -F "file=@audio.wav" \
  -F "model=<model-name>" \
  -F "response_format=json"
```

Python SDK:
```python
from openai import OpenAI
client = OpenAI(base_url="http://localhost:<port>/v1", api_key="not-needed")
result = client.audio.transcriptions.create(
    model="<model-name>",
    file=open("audio.wav", "rb"),
)
print(result.text)
```

---

## Recommendation Matrix

| Use case | Best option | Why |
|---|---|---|
| **Fastest transcription** | Parakeet-TDT-0.6B v2 (marcpope Docker) | 86ms, GPU, proven |
| **Coexist with llama-swap** | Moonshine base (CPU) | 309ms, 0 GB VRAM, always-on |
| **Best accuracy** | IBM Granite 4.0 1B Speech | #1 ASR leaderboard, 780ms, 4.3 GB |
| **Multilingual + formatting** | Qwen3-ASR-1.7B | 52 languages, proper casing/punctuation |
| **Real-time streaming** | Nemotron Streaming 0.6B (not tested) | Purpose-built, configurable latency |
| **Speaker diarization** | VibeVoice-ASR (not tested) | ASR + diarization in one model |
| **99-language coverage** | Whisper large-v3 (faster-whisper Docker) | Broadest language support |
| **ColdVox wave-1 HTTP container default** | Parakeet CPU on cluster (`http://192.168.30.207:5092`; local compose on `localhost:5092` as fallback) | Canonical first-class backend profile; OpenAI-compatible `/v1/audio/transcriptions` + `/health` |
| **Deferred comparison profiles** | Moonshine, Granite, Qwen3-ASR, Voxtral | Useful benchmarks and follow-on options, but not first-wave default obligations |

---

## Port Allocation

| Port | Service | Type | Status |
|---|---|---|---|
| 5092 | Parakeet CPU | Cluster `192.168.30.207` (apps/parakeet, achetronic image); local Docker fallback | **Primary** |
| 5093 | IBM Granite 4.0 1B | Native Python | **Installed** |
| 5094 | Qwen3-ASR-1.7B | Native Python | **Installed** |
| 5095 | Voxtral-Mini-4B | Native Python | **Installed** |
| 5096 | Moonshine base/tiny | Native Python | **Installed** |
| 8000 | Speaches | Docker | Available |
| 8100 | faster-whisper-server | Docker | Available |
| 8200 | Parakeet GPU (marcpope) | Docker | Available |
| 8003 | whisperx-blackwell | Docker | Available |
| 9000 | NIM Parakeet | Docker | **Broken on SM120** |

---

## Server File Locations

| Model | Server Script | Venv |
|---|---|---|
| IBM Granite 4.0 1B | `D:\LocalLargeLanguageModels\stt-eval\granite\server.py` | `granite\venv\` |
| Qwen3-ASR-1.7B | `D:\LocalLargeLanguageModels\stt-eval\qwen3-asr\server.py` | `qwen3-asr\venv\` |
| Voxtral-Mini-4B | `D:\LocalLargeLanguageModels\stt-eval\voxtral\server.py` | `voxtral\venv\` |
| Moonshine | `D:\LocalLargeLanguageModels\stt-eval\moonshine\server.py` | `moonshine\venv\` |
| Native Parakeet | `D:\LocalLargeLanguageModels\parakeet-stt\app.py` | `parakeet-stt\venv\` |

---

## Sources

- [HuggingFace Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard)
- [IBM Granite 4.0 1B Speech](https://huggingface.co/ibm-granite/granite-4.0-1b-speech)
- [NVIDIA Nemotron Speech Streaming](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)
- [Microsoft VibeVoice-ASR-HF](https://huggingface.co/microsoft/VibeVoice-ASR-HF)
- [Mistral Voxtral-Mini-4B-Realtime](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- [Mistral Voxtral Small 24B](https://huggingface.co/mistralai/Voxtral-Small-24B-2507)
- [Qwen3-ASR](https://github.com/QwenLM/Qwen3-ASR)
- [UsefulSensors Moonshine](https://github.com/usefulsensors/moonshine)
- [OpenAI Whisper](https://github.com/openai/whisper)
- ChatGPT PDF: "Best English Speech-to-Text Models for Self-Hosting on an RTX 5090" (March 25, 2026)
