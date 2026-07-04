# OpenFlow — Engineering Design Document

**A fully local, open-source Wispr Flow alternative powered by Ollama**

Version 1.0 — July 2026
Status: Design / pre-implementation
Target platforms: Windows 11 (primary), macOS, Linux (X11 + Wayland)

---

## Table of Contents

1. [Wispr Flow: What Is Known vs. Inferred](#1-wispr-flow-research)
2. [The Full Pipeline, Stage by Stage](#2-pipeline-breakdown)
3. [Local AI Stack (Ollama)](#3-local-ai-stack)
4. [Speech Recognition Engine Selection](#4-speech-recognition)
5. [System Architecture](#5-architecture)
6. [UI Design](#6-ui-design)
7. [Context Awareness](#7-context-awareness)
8. [Typing Injection](#8-typing-injection)
9. [Coding Mode](#9-coding-mode)
10. [Voice Commands](#10-voice-commands)
11. [Performance Optimization](#11-performance)
12. [Tech Stack](#12-tech-stack)
13. [Project Structure](#13-project-structure)
14. [Implementation Roadmap](#14-roadmap)
15. [Fable 5 Development Plan](#15-fable-5-plan) → see [FABLE5_PROMPT_PLAN.md](FABLE5_PROMPT_PLAN.md)

---

## 1. Wispr Flow Research

### 1.1 What is publicly KNOWN (from Wispr's own blog, docs, and press)

| Fact | Source |
|---|---|
| **Cloud-based.** All ASR and LLM processing happens on remote servers (Baseten-hosted models on AWS us-east-1). There is no offline mode. | Wispr docs, Baseten case study |
| **~700 ms p99 end-to-end latency** target: <200 ms ASR inference + <200 ms LLM inference + ≤200 ms network, per their engineering blog. | wisprflow.ai/post/technical-challenges |
| **Context-conditioned ASR.** Their models are conditioned on speaker characteristics, surrounding on-screen text, and per-user history to resolve ambiguous audio. | Wispr engineering blog |
| **Context capture.** During dictation, Flow sends nearby text, proper nouns extracted from on-screen content, app metadata, and (in IDEs) variable/file names to the cloud to bias transcription. | Wispr Help Center, "Context Awareness" |
| **LLM formatting with token-level control** to match personal writing style (dash vs. comma habits, capitalization). They note "LLMs are phenomenal at recall, but very low precision" — i.e., they constrain the LLM rather than letting it free-rewrite. | Wispr engineering blog |
| **Learning from corrections.** Device-level edits are captured to train a per-user preference policy (they describe it as a local RL-style loop). | Wispr engineering blog |
| **Push-to-talk UX.** Hold a hotkey, speak, release → formatted text appears in the focused app. There is also a hands-free toggle mode. | Product docs |
| **Windows app is Electron**; the macOS app is native (and noticeably more polished). Windows needs only system microphone permission — no per-app accessibility permission. macOS requires Accessibility + Microphone permissions. | Wispr docs, user reports |
| **Android uses an AccessibilityService** to detect focused text fields and insert text — confirming their general pattern: accessibility APIs for focus detection + insertion. | Wispr docs |
| **Features:** auto punctuation/capitalization, filler-word removal, auto-formatting (lists, paragraphs), tone matching per app, personal dictionary, voice-triggered snippets, whisper-voice support, 100+ languages, Command Mode (experimental — voice-driven editing of selected text), and a developer mode that recognizes code syntax and integrates with Cursor/Windsurf/GitHub contexts. | wisprflow.ai/features, Help Center |
| **~800 MB RAM idle, ~8% background CPU** on Windows (user measurements) — an Electron app with persistent audio + network stack. | Independent reviews |
| **Scale:** ~1B words dictated/month (2026). | Wispr blog |

### 1.2 What is INFERRED (educated engineering reverse-engineering)

- **Pipeline shape.** Given the latency budget, they almost certainly run: local VAD + audio capture → opus/pcm stream to server *while you speak* → streaming ASR producing partials → on hotkey release, a final ASR pass + a single LLM formatting call over the whole utterance → response injected as one block. The "one-shot final format" explains why Flow inserts a complete polished block rather than streaming words into your app.
- **Injection method.** On Windows, Flow inserts text nearly instantly even for long paragraphs, which is characteristic of **clipboard-swap + Ctrl-V simulation** (save clipboard → set text → send paste → restore clipboard), with `SendInput` Unicode events as fallback for apps that block paste. Reports of Flow "freezing target apps" (VS Code) are consistent with clipboard/paste timing races from an Electron process. On macOS, Accessibility (AXUIElement `AXSelectedText` insertion) + CGEvent paste fallback is the standard pattern.
- **Context reading.** On macOS, the Accessibility tree (`AXValue`, `AXSelectedText`, `AXFocusedUIElement`) provides nearby text; on Windows, **UI Automation** (`TextPattern`, `ValuePattern`) plus window title/process name. "Proper nouns extracted from screen" implies they run entity extraction over the captured text and feed it to the ASR as a bias list (equivalent to Whisper's `initial_prompt` / hotword biasing).
- **Dictionary implementation.** Almost certainly ASR hotword-biasing/prompting + a post-ASR fuzzy-replace pass + inclusion of dictionary terms in the LLM prompt. This triple layer explains why dictionary words survive even when badly mispronounced.
- **Snippets** are trigger-phrase detection in the *transcript* (not audio): after ASR, a matcher looks for registered trigger phrases and expands them before/instead of LLM formatting.
- **Filler removal, punctuation, corrections** ("actually, scratch that") are LLM-prompt behaviors, not ASR features — the LLM receives the raw verbatim transcript plus context and rewrites it under strict instructions.
- **Style learning** is most plausibly few-shot: recent samples of the user's actual sent text per app (captured at insertion/edit time) are included in the formatting prompt, plus learned per-user rules ("never use em-dashes").
- **Hotkey handling** is a low-level keyboard hook (Win32 `WH_KEYBOARD_LL` / macOS CGEventTap) rather than `RegisterHotKey`, because Flow supports *hold* semantics (key-down starts, key-up stops) and modifier-only bindings (Fn, Ctrl alone) which RegisterHotKey cannot express.
- **Background service architecture:** a tray-resident process owning audio + hotkeys + injection, with the settings window as a separate renderer — standard Electron main/renderer split.

**Bottom line:** nothing in Wispr Flow requires the cloud *except* their large proprietary models and cross-device personalization. Every mechanism — VAD, streaming ASR, LLM cleanup, context capture, injection — has a mature open-source local equivalent. The engineering challenge is hitting their latency numbers on consumer hardware, which is now feasible (see §4, §11).

---

## 2. Pipeline Breakdown

```
 Microphone ─► Audio Capture ─► Resample ─► VAD ─► Ring Buffer
                                                      │
                              ┌───────────────────────┘
                              ▼
                    Streaming ASR (partials)          ── live preview in HUD
                              │  (hotkey release)
                              ▼
                    Final ASR pass (whole utterance)
                              ▼
                    Deterministic post-processing      ── dictionary, snippets,
                              │                           spoken-punctuation, ITN
                              ▼
                    Intent gate: dictation | command | snippet
                              ▼
                    Context snapshot (app, window, nearby text)
                              ▼
                    LLM cleanup & formatting (Ollama)
                              ▼
                    Output guard (diff sanity check)
                              ▼
                    Injection (clipboard-paste / SendInput / AX)
                              ▼
                    User's focused application
```

### Stage 1 — Audio capture
- **Purpose:** low-latency mono PCM from the selected device.
- **Libraries:** `cpal` (Rust, WASAPI/CoreAudio/ALSA), alternatives: `miniaudio`, PortAudio, `sounddevice` (Python).
- **Design:** capture at device native rate, convert to 16 kHz mono f32 with `rubato`. 10–30 ms callback buffers. Keep the stream **always open but paused** (or capture-and-discard) — opening a WASAPI stream cold costs 100–300 ms you can't afford on key-down.
- **Tradeoffs:** exclusive-mode WASAPI lowers latency but steals the mic; shared mode is correct here. Latency contribution: ~10–30 ms.

### Stage 2 — Voice Activity Detection
- **Purpose:** trim leading/trailing silence, gate the ASR (don't transcribe keyboard noise), enable hands-free mode end-pointing.
- **Libraries:** **Silero VAD v5** via ONNX Runtime (`ort`/`vad-rs` in Rust) — de-facto standard, ~1 ms per 30 ms frame on CPU. Alternatives: WebRTC VAD (fast, less accurate), TEN-VAD (newer, comparable).
- **Design:** run on every 32 ms frame; in push-to-talk, VAD only trims edges; in hands-free mode, 500–800 ms of trailing silence triggers finalization.
- **Tradeoffs:** aggressive VAD clips soft speech onset — keep 250 ms pre-roll from the ring buffer.

### Stage 3 — Audio buffering
- **Purpose:** decouple capture callback (real-time thread, must never block) from inference.
- **Design:** lock-free SPSC ring buffer (~30 s capacity). Pre-roll: buffer continuously even before key-down so the first syllable (often spoken 50–150 ms before the key registers) isn't lost.

### Stage 4 — Streaming ASR
- **Purpose:** partial transcripts while speaking (HUD preview + warm decode state), plus a high-quality final pass.
- **Libraries:** see §4. Streaming strategies:
  - **LocalAgreement-2** (ufal/whisper_streaming, WhisperLiveKit): re-decode a growing window; emit the prefix that two consecutive decodes agree on. Works with any Whisper backend; adds 1–3 s to *confirmed* words but partials feel live.
  - **Chunked re-decode** (simpler): every 500 ms re-transcribe the last N seconds, show the result as a volatile partial, never commit. This is what most local dictation apps ship.
  - **Natively streaming models** (Parakeet TDT, Moonshine v2 streaming encoder, Kyutai STT): true frame-synchronous output, 50–300 ms partial latency, no agreement hack needed.
- **Recommended:** two-tier — natively streaming model for partials where available; **always re-run one final pass over the full utterance** with the dictionary prompt on hotkey release. Utterances are short (2–30 s), so the final pass costs 100–400 ms and buys maximal accuracy plus consistent punctuation. The final pass is also where `initial_prompt` biasing (dictionary, context nouns) is applied.

### Stage 5 — Deterministic post-processing (before the LLM)
- **Purpose:** everything that must be *reliable* should not depend on an LLM.
- Steps, in order:
  1. **Dictionary fuzzy-replace** — phonetic/edit-distance match of ASR output against user dictionary (e.g., "cube control" → "kubectl").
  2. **Snippet trigger detection** — exact/normalized match of trigger phrases → expand and short-circuit the LLM entirely.
  3. **Spoken punctuation & controls** — "new line", "new paragraph", "period" (optional, off by default since the LLM handles punctuation).
  4. **Inverse text normalization** — numbers, currency, dates ("twenty five dollars" → "$25"). Library: NeMo ITN grammars or a compact rule set.
- **Tradeoffs:** doing this pre-LLM makes behavior deterministic and testable; the LLM then only has to *not break* these substitutions (prompt: "preserve technical terms exactly").

### Stage 6 — Intent gate
- **Purpose:** decide dictation vs. voice command vs. snippet, in <5 ms.
- **Design:** rules first (regex/keyword over the transcript head: "rewrite…", "summarize…", or a selection is active + imperative verb), tiny-LLM classification only for ambiguous cases (see §10). Never send plain dictation through a classifier round-trip — that's wasted latency.

### Stage 7 — Context snapshot
- Captured at **key-down** (not release — parallelize with speaking): process name, window title, focused-control text via UIA/AX, current selection, app profile (email/chat/IDE/doc). Feeds both ASR biasing and the LLM prompt. Details in §7.

### Stage 8 — LLM cleanup (Ollama)
- **Purpose:** filler removal, grammar, punctuation polish, paragraphing, list detection, self-correction resolution ("actually, make that Tuesday"), tone matching, dictionary enforcement.
- **Design:** single chat completion, temperature 0–0.2, streaming enabled (for optional progressive injection), strict system prompt + few-shot examples + JSON-free plain-text output (structured output only for command interpretation). Keep the model **loaded** (`keep_alive: -1`) and pre-warm at startup.
- **Skip-LLM fast path:** utterances under ~4 words with no filler markers can bypass the LLM entirely (regex confirms no "um/uh"), saving 150–400 ms on quick commands like a filename.
- **Output guard:** reject/deLLM the result if length deviates wildly (>2.5× or <0.3× input), if it contains meta-text ("Here is the cleaned text"), or if dictionary terms were mangled — fall back to the deterministic transcript. This is the "low precision" problem Wispr describes, solved with a guardrail instead of trust.

### Stage 9 — Injection
- Clipboard-paste primary, SendInput/CGEvent/AX fallbacks. Details in §8. Latency: 20–60 ms.

**End-to-end budget (hotkey release → text visible), GPU-equipped machine:**

| Stage | Target |
|---|---|
| Final ASR pass (utterance ≤15 s) | 120–300 ms |
| Post-processing + intent + context (parallelized) | <10 ms |
| LLM cleanup (~150 tok in / 60 tok out, 4B model) | 150–350 ms |
| Injection | 20–60 ms |
| **Total** | **≈300–700 ms** (≤500 ms typical) |

---

## 3. Local AI Stack

All text intelligence runs through **Ollama** (`http://localhost:11434/api/chat`). Design principle: **the smallest model that does the job, kept resident in VRAM**. Two models max resident simultaneously (cleanup + command), configured via `OLLAMA_MAX_LOADED_MODELS=2`, `keep_alive: -1`.

### Model landscape (as of mid-2026)

| Family | Strengths | Weaknesses for this app |
|---|---|---|
| **Qwen 3 / 3.6** (0.6B–32B, dense + MoE) | Best instruction-following per parameter; excellent multilingual; `/no_think` mode for low latency; strong JSON/structured output | Thinking mode must be explicitly disabled or it destroys latency |
| **Gemma 3 / Gemma 4** (1B–27B) | Very strong prose quality and tone; great multilingual (140+ langs); good at "sound like a human" rewriting | Slightly weaker at strict instruction compliance in small sizes; no system-prompt fine-grained control in some builds |
| **Llama 3.x / 4** | Broad ecosystem, safe default | Small variants (1–3B) now outclassed by Qwen3/Gemma3 at same size |
| **DeepSeek (R1-distill, V3/V4 family)** | Superb reasoning | Reasoning = latency; wrong tool for text cleanup |
| **Mistral (Ministral 3B/8B, Small 3.x)** | Fast, good European-language quality | Fewer size options; mid sizes outperformed by Qwen 3.6 |
| **gpt-oss:20b** (MoE) | 7B-class speed with near-20B quality; runs in 16 GB | Overkill for cleanup; great for commands/rewrites |
| **qwen3-coder:30b** (MoE, 3.3B active) | Best local coding model per GB; 176 tok/s class | Only needed for coding mode |

### Task → model assignment

| Task | Primary (≥12 GB VRAM) | Fallback (8 GB / CPU) | Notes |
|---|---|---|---|
| **Speech cleanup + punctuation + fillers + formatting** (the hot path) | `qwen3:4b-instruct` (non-thinking) | `qwen3:1.7b` or `gemma3:1b` | Must sustain >100 tok/s; temp 0.1 |
| **Grammar fixing** | same model, same prompt | same | Folded into cleanup — never a separate call |
| **Command interpretation** (intent → structured action) | `qwen3:4b-instruct` w/ JSON schema (Ollama structured outputs) | `qwen3:1.7b` | Only invoked when the intent gate is unsure |
| **Rewriting / tone / email mode** | `gemma3:12b` or `gpt-oss:20b` | `gemma3:4b` | Quality-sensitive, latency-tolerant (user asked for a rewrite; 1–2 s is fine) |
| **Summarization / expand / translate** | `gpt-oss:20b` or `qwen3.6` mid-size | `qwen3:8b` | |
| **Coding mode** (identifier casing, code-aware cleanup) | `qwen3-coder:30b` if VRAM allows, else `qwen3:4b` + rules | rules only | Most casing work is deterministic (§9); LLM is assist, not core |
| **Voice assistant Q&A ("ask local AI")** | user-selectable, default `gpt-oss:20b` | `qwen3:8b` | Streams answer into a popover, not injected |

**Why not one big model for everything?** The cleanup call runs on *every* utterance; its latency defines the product feel. A 4B model at Q4 emits 60 output tokens in ~200 ms on an RTX 3060-class GPU; a 20B MoE takes ~450 ms and a 27B dense >1 s. Route by task, keep the hot-path model tiny and resident.

**Prompt design for the hot path (critical learnings):**
- Verbatim transcript in, cleaned text out, *nothing else* — forbid preambles explicitly and strip them defensively.
- Include: app profile ("You are formatting text for Slack — casual, no sign-off"), dictionary terms ("preserve exactly: kubectl, Sagar, PostHog"), up to ~200 chars of preceding text from the field (for tense/case continuity), and 2–3 few-shot pairs.
- Instruct: remove fillers and false starts; apply self-corrections ("no wait, Tuesday" → keep only Tuesday); do not add content; do not answer questions — *transcribe* them. (The classic failure: user dictates "what time is the meeting?" and the LLM answers it.)

---

## 4. Speech Recognition

### Engine comparison

| Engine | WER (EN) | Latency (10 s audio) | RAM/VRAM | GPU | CPU | Apple Silicon | Win | Linux | Languages | Streaming |
|---|---|---|---|---|---|---|---|---|---|---|
| **faster-whisper large-v3-turbo** (CTranslate2) | ~7.4% open-bench / ~2% clean | 150–400 ms GPU; 2–5× RT CPU | ~1.5 GB (int8) | CUDA excellent | good (int8) | via CTranslate2 CPU (no Metal) | ✅ | ✅ | ~99 | via re-decode |
| **whisper.cpp (large-v3-turbo / distil)** | same weights | similar; best-in-class Metal | 1–2 GB | CUDA/Vulkan/Metal | very good | **excellent (Metal/CoreML)** | ✅ | ✅ | ~99 | via re-decode |
| **NVIDIA Parakeet TDT 0.6B v3** | **6.3%** (beats large-v3) | ~30–50 ms; 50–100× RT CPU | ~1.5 GB fp16 / 700 MB int8 | 2800×RT | 50–100× RT | ✅ (ONNX/MLX ports, parakeet.cpp) | ✅ (ONNX Runtime) | ✅ | 25 (Eur.) | **native (TDT)** |
| **Moonshine v2 (small/medium)** | ≈ large-v3 at medium | 50–260 ms | 100–600 MB | ✅ | excellent | ✅ | ✅ | ✅ | EN-focused (+few) | **native streaming encoder** |
| **SenseVoice-Small** | good ZH/EN | ~70 ms (non-autoregressive, 15× faster than large) | ~1 GB | ✅ | ✅ | via ONNX | ✅ | ✅ | 50+ (best ZH/JA/KO) | chunked |
| **Distil-Whisper (distil-large-v3.5)** | ≈ large-v3 EN | ~2× faster than turbo | ~750 MB | ✅ | ✅ | via whisper.cpp | ✅ | ✅ | EN only | via re-decode |
| **MLX Whisper** | same weights | fast on M-series | — | Metal | — | **native** | ❌ | ❌ | ~99 | via re-decode |
| **Whisper large-v3 (reference)** | 7.4% | 0.5–1.5 s GPU | ~3 GB | ✅ | slow | via ports | ✅ | ✅ | ~99 | ❌ |

### Recommendation

**Ship an engine abstraction (`SttEngine` trait) with two first-class backends:**

1. **Default / English & European languages: Parakeet TDT 0.6B v3 via ONNX Runtime** (or `parakeet.cpp`). Best accuracy *and* 10× the speed of Whisper; natively streaming (true partials at <100 ms); runs 50×+ real-time on CPU alone, which means **the full experience works on GPU-less laptops**. This is the single biggest lever for the sub-500 ms goal.
2. **Multilingual / fallback: whisper.cpp with `large-v3-turbo` (quantized q5) + `distil-large-v3.5` for EN-only CPU boxes.** whisper.cpp over faster-whisper because it's a C library that embeds cleanly in a Rust/Tauri process (no Python sidecar), has Vulkan (any GPU on Windows) + Metal + CUDA, and identical weights availability. If we later ship a Python transcription sidecar anyway, faster-whisper becomes attractive for its cleaner batch API — but embedding wins for a desktop product.

Per-language routing: user picks "auto" → language detected from first second of audio (Whisper's built-in LID or a fastText pass on the partial) → route EN/Eur to Parakeet, everything else to Whisper turbo. Dictionary biasing: Whisper `initial_prompt`; Parakeet v3 supports context biasing lists via NeMo-style hotword boosting (and our post-ASR fuzzy-replace covers both regardless).

---

## 5. Architecture

Single **Tauri 2** application: one Rust core process (tray-resident daemon — owns everything real-time) + WebView windows (settings UI, HUD overlay). No separate daemon process in v1 — Tauri's core *is* the daemon; the settings window is closed/hidden, not the app. A `--headless` CLI mode runs the core without any window for power users.

```
┌────────────────────────────────────────────────────────────────┐
│  Rust Core (tray daemon)                                       │
│                                                                │
│  HotkeyListener (low-level hook: rdev/global-hotkey)           │
│        │ key-down/up events                                    │
│        ▼                                                       │
│  SessionOrchestrator  ◄─── single mpsc event loop (the only    │
│        │                    owner of session state; Handy-style │
│        │                    single-threaded coordinator)        │
│        ├─► AudioEngine (cpal + rubato + ring buffer)           │
│        ├─► VadService (Silero v5 / ort)                        │
│        ├─► TranscriptionService (SttEngine trait:              │
│        │      ParakeetOnnx | WhisperCpp)                       │
│        ├─► ContextManager (UIA/AX/AT-SPI snapshot, app         │
│        │      profiles, focused-control text)                  │
│        ├─► PostProcessor (dictionary, snippets, ITN, intent)   │
│        ├─► OllamaClient (chat/stream, model routing,           │
│        │      keep-alive, structured outputs, health check)    │
│        ├─► PromptPipeline (template engine, per-profile        │
│        │      prompts, few-shot store, output guard)           │
│        ├─► InjectionService (clipboard-paste | SendInput |     │
│        │      AXUIElement | XTEST/wtype; per-app strategy)     │
│        └─► HistoryStore (SQLite: transcripts, timings,         │
│               corrections; local only)                         │
│                                                                │
│  SettingsManager (JSON file + SQLite, hot-reload, profiles)    │
│  ModelManager (download STT models w/ checksums; verify        │
│     Ollama models present; pull on demand)                     │
│  PluginHost (v2: WASM or JSON-RPC subprocess plugins that      │
│     register context providers / post-processors / commands)   │
│  Logging: tracing → rotating local files; per-stage spans      │
│     feed the latency monitor. Telemetry: NONE (no network      │
│     calls except localhost Ollama + opt-in model downloads).   │
└──────────────┬─────────────────────────────────────────────────┘
               │ Tauri IPC (events + commands)
   ┌───────────┴───────────┐
   │ Settings window (React)│   HUD overlay window (always-on-top,
   │ tray menu              │   transparent: waveform, partials,
   └────────────────────────┘   state pill: listening/thinking/done)
```

**Key decisions:**
- **Single-threaded orchestrator over an mpsc channel** — all state transitions (idle → recording → transcribing → formatting → injecting) serialize through one loop. This is the proven pattern (Handy uses it) that eliminates the hotkey/audio/async race conditions that plague dictation apps.
- **Everything behind traits** (`SttEngine`, `Injector`, `ContextProvider`, `LlmBackend`) so engines are swappable and testable with fakes.
- **Profiles** = named bundles of {hotkey, STT model, LLM model, prompt template, injection strategy, language} with per-app auto-activation rules ("when process == Cursor.exe → Coding profile").
- **Config:** `config.json` (human-editable, versioned schema, hot-reload via file watch); SQLite for dictionary/snippets/history.
- **Telemetry disabled by design:** the settings UI shows a network-activity indicator asserting "0 external connections"; CI has a test that the binary makes no non-localhost requests.

---

## 6. UI Design

Design language: quiet, native-feeling, tray-first. The app is invisible 99% of the time; the HUD is the product.

**HUD overlay (during dictation):** small centered-bottom pill — mic waveform, live partial transcript (last ~8 words), state color (red recording / amber formatting / green injected), and per-stage latency readout in dev mode. Click-through, always-on-top, per-monitor DPI aware.

**Settings window — left-nav sections:**
1. **General** — launch at login, hotkey capture widget (supports hold vs. toggle, modifier-only keys), language, active profile.
2. **Microphone** — device picker with live level meter, input gain, test recording playback.
3. **Models** — STT engine/model picker with download manager (size, checksum, progress); Ollama status card (running? which models pulled? one-click `ollama pull` for missing); per-task model routing table.
4. **Prompt editor** — per-profile system prompt with template variables (`{app_name}`, `{dictionary}`, `{context}`, `{tone}`), few-shot example editor, live "test with sample transcript" button showing before/after and token counts.
5. **Snippets** — trigger phrase → expansion (plain or template with cursor placeholder), per-profile scoping.
6. **Dictionary** — word list with optional pronunciation hint ("kubectl — sounds like cube-cuttle"), import/export, auto-suggestions harvested from user corrections.
7. **History & Logs** — searchable transcript history (raw vs. formatted diff view), structured log viewer, "copy debug bundle".
8. **Latency monitor** — per-stage waterfall (audio→VAD→ASR→LLM→inject) for the last N dictations, p50/p95 charts, model tok/s.
9. **Developer** — coding-mode settings, per-app profile rules editor, plugin manager, raw event stream console.
10. **Profiles & Themes** — profile CRUD, light/dark/system, accent color, HUD position/size.

---

## 7. Context Awareness

Wispr Flow captures "nearby text, proper nouns from on-screen content, app metadata, and in coding environments variable and file names" (their docs). Local recreation, in layers from cheap to rich:

**Layer 1 — App identity (always, <1 ms):** foreground process name + window title. Windows: `GetForegroundWindow` → `GetWindowThreadProcessId` → process image name. macOS: `NSWorkspace.frontmostApplication`. Linux: `_NET_ACTIVE_WINDOW` (X11) / compositor protocols or `swaymsg`/D-Bus (Wayland — often unavailable; degrade gracefully).
Window titles are context gold: `"Re: Q3 budget — Gmail — Chrome"` gives you app = email + subject; `"main.rs — wisprflow-ai-clone — Cursor"` gives file + project + IDE.

**Layer 2 — App profile classification (rules table):** map process/title patterns → profile: `slack.exe|discord.exe` → chat (casual, no sign-offs); `outlook|thunderbird|title contains "- Gmail"` → email (greeting/sign-off aware, formal); `code.exe|cursor.exe|windsurf|jetbrains|terminal|wt.exe` → coding; `winword|"- Google Docs"|notion` → document (paragraphs, headings). Browser tabs are classified by title suffix since the process is just chrome.exe. User-extensible in Developer settings.

**Layer 3 — Focused-control text (the "nearby text"):**
- **Windows:** UI Automation — `IUIAutomation::GetFocusedElement`, then `TextPattern` (`DocumentRange.GetText`, caret via `GetSelection`) or `ValuePattern.Value`. Works in Word, browsers (Chromium/Firefox expose UIA), Electron apps, WinUI. Fails in most terminals and games → skip.
- **macOS:** AX API — `AXUIElementCopyAttributeValue` for `AXFocusedUIElement` → `AXValue`, `AXSelectedText`, `AXSelectedTextRange`. Works broadly; Electron apps expose AX trees.
- **Linux:** AT-SPI2 via D-Bus (GTK/Qt apps); very patchy on Wayland → Layer 1/2 only.
- Budget: capture asynchronously at **key-down** with a 150 ms timeout; never block the pipeline on it. Truncate to ~1,000 chars around the caret.
- **Selection capture:** if text is selected, record it — it's the operand for voice commands ("rewrite this professionally").
- **Entity extraction:** regex + capitalization heuristics over captured text → proper-noun list → ASR bias prompt + LLM "preserve these" list. No NER model needed in v1.

**Layer 4 — IDE deep context (coding mode, v2):** a tiny optional VS Code/Cursor extension exposing `{file, language, symbols in scope, cursor line}` over a localhost socket — far richer than UIA scraping and how "IDE integration" should be done. Terminal detection: process ancestry (wt.exe/kitty/iterm) → coding profile with shell-command formatting.

**Privacy stance:** context text lives in memory for the session only, is never persisted (history stores transcripts, not screen content), and never leaves the machine — trivially true since there's no server.

---

## 8. Typing Injection

The hardest "boring" problem in this product. Strategy pattern with per-app overrides and automatic fallback chain.

### Windows
| Method | Pros | Cons |
|---|---|---|
| **Clipboard swap + Ctrl-V** (`SetClipboardData` → `SendInput` Ctrl-V → restore) | Instant for any length; preserves Unicode/emoji; works in 99% of apps | Clobbers clipboard briefly (must save/restore incl. formats); paste-blocked fields (some password/terminal); timing races with slow apps (Teams/Outlook) — need paste-completion delay before restore |
| **`SendInput` with `KEYEVENTF_UNICODE`** | No clipboard touch; works where paste is blocked; enables word-by-word streaming | Slow for long text (per-char events); some apps (games, admin-elevated windows) drop events; known jitter with rapid Unicode bursts |
| **UI Automation `ValuePattern.SetValue` / `TextPattern`** | Atomic, clean, no input queue | Replaces whole value (caret handling painful); many apps don't implement the pattern writable; wrong tool for insertion-at-caret |
| ~~WM_CHAR / WM_SETTEXT~~ | — | Bypasses input stack; breaks in modern apps; don't |

**Windows plan:** clipboard-paste primary (with format-preserving save/restore and a configurable 50–150 ms restore delay), SendInput-Unicode fallback (auto-selected for terminals & paste-blockers via the per-app table), UIA read-only for context. Elevated-window injection fails by design (UIPI) → detect and show a hint.

### macOS
- **CGEvent keyboard events** (`CGEventCreateKeyboardEvent` + `keyboardSetUnicodeString`): the workhorse; needs Accessibility permission.
- **Paste via CGEvent Cmd-V** + NSPasteboard swap: same tradeoffs as Windows, primary for long text.
- **AX direct insertion** (`AXSelectedText` set on focused element): cleanest when supported; patchy in Electron/web views.
- Plan: paste primary, CGEvent-Unicode fallback, AX opportunistic.

### Linux
- **X11:** XTEST (`xdotool type`) — reliable; clipboard paste via XCB + synthetic Ctrl-V.
- **Wayland:** the hard case. Options: `wtype` (virtual-keyboard protocol, wlroots compositors), `ydotool` (uinput, needs daemon/root once), or clipboard + user-pressed paste as last resort. GNOME lacks virtual-keyboard → ydotool/uinput path. Abstract behind the `Injector` trait; `enigo` crate covers X11/Windows/macOS baseline.

**Cross-cutting:** never inject if the foreground window changed since capture started (re-check + focus-restore option); undo support = remember last injection length → single-shot "undo dictation" command sends the right number of backspaces or Ctrl-Z.

---

## 9. Coding Mode

Auto-activated by app profile (IDE/terminal). Two cooperating layers:

**Deterministic layer (rules, always on, testable):**
- **Casing engine:** spoken forms → identifiers. "get user by ID, camel case" → `getUserById`; explicit commands ("snake case", "pascal case", "kebab", "screaming snake", "dot case") and *learned defaults per language* (Rust profile defaults snake_case for functions, PascalCase for types). Implementation: tokenize the phrase, apply casing transform; dictionary of untouchables (`kubectl`, `npm`, `PostgreSQL`).
- **Path/file grammar:** "src slash components slash button dot TSX" → `src/components/Button.tsx` (slash/dot/dash/underscore spoken tokens; extension casing table).
- **Symbol vocabulary:** "arrow" → `=>` / `->` by language, "double equals", "pipe", "ampersand", "backtick", "open brace"…
- **Terminal grammar:** "git commit dash M" → `git commit -m`; common CLI lexicon (flags, pipes, redirects).
- **Markdown/code blocks:** "code block python … end code block" → fenced block; "bullet list: a, b, c" → markdown list.

**LLM layer (`qwen3-coder:30b` when available, else the hot-path model with a coding prompt):** resolves ambiguity the rules can't ("call the fetch users function with the current user's ID" → `fetchUsers(currentUser.id)` given IDE symbol context from §7 Layer 4), formats dictated prose comments/docstrings, and never runs for plain identifier dictation (rules handle it at 0 ms).

**IDE awareness:** language from window title extension (`.rs`, `.py`) or IDE extension socket; per-language casing defaults; "insert as comment" honors language comment syntax. In coding profile the cleanup prompt flips: *don't* prettify prose, *don't* touch symbols, preserve exact spacing, no smart quotes ever.

---

## 10. Voice Commands

**Grammar:** commands operate on either the **current selection** (captured in the context snapshot) or the **preceding dictation** (last injection). Two invocation styles:
1. **Command-only utterance** (push-to-talk while text is selected): "rewrite this professionally", "make it shorter", "translate to German", "fix grammar", "turn this into an email", "bullet list", "convert to markdown", "summarize".
2. **Inline suffix** (v2): "…and we'll ship Friday — *make that sound professional*".

**Detection pipeline (latency-tiered):**
- **Tier 0 (regex, <1 ms):** utterance matches `^(rewrite|summarize|shorten|expand|translate|fix|convert|make (it|this|that))\b` *and* (selection exists or last-injection exists) → command.
- **Tier 1 (LLM classify+parse, one call):** ambiguous cases go to the hot-path model with **Ollama structured outputs** (JSON schema): `{intent: dictate|transform|assistant, action?: rewrite|summarize|shorten|expand|translate|to_email|fix_grammar|to_bullets|to_markdown|explain, args?: {language?, tone?}}`. One call does classification *and* argument extraction.
- **Execution:** `transform` → route to the quality model (§3) with `{action-specific prompt + selected text}` → replace selection via injection. `assistant` ("ask local AI: …", "explain this") → stream answer into a popover window (never inject into the user's app uninvited).
- **Registry-driven:** commands are data (name, trigger patterns, prompt template, target model, output mode: replace/insert/popover) stored in config → users add custom commands in the UI; plugins can register more.
- **Safety:** transforms always show a 1-line flash in the HUD ("Rewrote 42 words — press ⌘Z to undo") and support the undo mechanism from §8.

---

## 11. Performance Optimization

Target: **≤500 ms p50, ≤800 ms p95** from hotkey-release to injected text.

1. **Do work while the user speaks.** Context snapshot at key-down; streaming ASR keeps the encoder warm and partials mean the final pass mostly confirms; the LLM prompt prefix (system + context + dictionary) can be sent for **prefill during speech** so only the transcript suffix remains to prefill at release (Ollama keeps per-request KV; achieve the same effect by keeping the prompt prefix stable → prompt-cache hits).
2. **Model residency.** `keep_alive: -1`; pre-warm STT and LLM with a dummy inference at startup; pin ONNX session. First-dictation-after-boot should not be 5 s.
3. **Streaming everywhere:** ASR partials (TDT frame-synchronous or 500 ms re-decode); LLM token streaming — optionally inject sentence-by-sentence as tokens arrive (off by default; on = perceived latency ~250 ms).
4. **Incremental decoding:** LocalAgreement-2 for Whisper backend; TDT is natively incremental. Cap re-decode window at 8–10 s with confirmed-prefix trimming.
5. **GPU:** CUDA for ONNX/whisper.cpp on NVIDIA; Vulkan backend for AMD/Intel on Windows; Metal on macOS. Auto-detect with CPU fallback and *tell the user* which path is active in the latency monitor.
6. **Quantization ladder:** STT int8/q5; LLM Q4_K_M default (2× speed of Q8, negligible quality loss for cleanup).
7. **Skip paths:** no-LLM fast path (§2 stage 8); snippet short-circuit; command Tier-0 regex.
8. **Batching is an anti-goal** — single user, latency-bound; never batch, never queue behind background jobs.
9. **Memory:** one STT model + one hot LLM resident ≈ 4–6 GB total on a 12 GB card with the 4B cleanup model; the quality model loads on demand (accepting 1–3 s swap the first time a rewrite is requested — Ollama handles LRU).
10. **Measure or it didn't happen:** every stage emits a tracing span; the latency monitor (§6) is built in phase 1, not bolted on.

---

## 12. Tech Stack

| Concern | Choice | Why (and runner-up) |
|---|---|---|
| Desktop framework | **Tauri 2** (Rust core + WebView UI) | 10–20 MB RAM idle vs Electron's 500+; Rust core *is* the real-time daemon; proven by Handy/SilentKeys. (Electron: easier but reproduces Wispr's 800 MB problem. Pure native ×3 platforms: too slow to build.) |
| Frontend | **React + TypeScript + Vite + Tailwind + shadcn/ui** | Boring, fast to build, huge component ecosystem. (Svelte fine too.) |
| Core language | **Rust** | cpal/ort/enigo/rubato ecosystem, fearless concurrency for the audio path |
| Audio | **cpal** + **rubato** | Cross-platform WASAPI/CoreAudio/ALSA |
| VAD | **Silero v5** via `ort` (vad-rs) | Standard, tiny, accurate |
| STT | **ONNX Runtime (Parakeet TDT v3)** + **whisper-rs (whisper.cpp bindings)** | See §4 |
| LLM | **Ollama HTTP API** (`/api/chat`, structured outputs, keep_alive) | User-requested; also the easiest model management story. Design `LlmBackend` trait so llama.cpp-server/LM Studio work too. |
| Hotkeys | **rdev / low-level hooks** (WH_KEYBOARD_LL, CGEventTap, evdev) wrapped in our listener; `global-hotkey` crate insufficient (no key-up/hold semantics) | Hold-to-talk requires down/up events |
| Injection | **enigo** + custom clipboard module (arboard) + windows-rs UIA + wtype/ydotool | See §8 |
| System info / focus | **windows-rs** (Win32+UIA), **objc2** (AX), **x11rb/zbus** | |
| Database | **SQLite (rusqlite)** — dictionary, snippets, history, few-shot store | Zero-ops, local |
| Settings | **JSON file + serde + schema version + file-watch hot reload** | Human-editable, diffable |
| Model management | Own downloader (reqwest + sha256 + resume) for STT; delegate LLMs to Ollama pull | |
| Packaging | **Tauri bundler**: NSIS (.exe installer) + MSI on Windows, .dmg + notarization on macOS, AppImage/.deb on Linux | |
| Auto-update | **Tauri updater** (signed manifests, GitHub Releases), opt-in | |
| CI | GitHub Actions matrix builds; cargo test + vitest; release signing | |

---

## 13. Project Structure

```
openflow/
├── Cargo.toml                 # workspace
├── apps/
│   └── desktop/               # Tauri app
│       ├── src-tauri/         # thin shell: tray, windows, IPC commands
│       └── ui/                # React settings + HUD (Vite)
├── crates/
│   ├── core/                  # SessionOrchestrator, state machine, events
│   ├── audio/                 # cpal capture, resampler, ring buffer, VAD
│   ├── stt/                   # SttEngine trait; parakeet-onnx; whisper-cpp;
│   │                          #   streaming policies (LocalAgreement)
│   ├── llm/                   # LlmBackend trait; ollama client; model router;
│   │                          #   prompt pipeline; output guard
│   ├── postproc/              # dictionary matcher, snippets, ITN, casing
│   │                          #   engine (coding mode), intent gate
│   ├── context/               # ContextProvider trait; win-uia; mac-ax;
│   │                          #   linux-atspi; app profiles; entity extract
│   ├── inject/                # Injector trait; clipboard; sendinput; cgevent;
│   │                          #   x11/wayland; per-app strategy table
│   ├── hotkey/                # low-level hook listener, chord parser
│   ├── settings/              # config schema, profiles, migrations, watch
│   ├── store/                 # SQLite: history, dictionary, snippets
│   ├── models/                # STT model registry, downloader, checksums
│   └── plugins/               # (v2) plugin host, manifest, WASM/JSON-RPC
├── tests/
│   ├── e2e/                   # scripted dictation → mock injector assertions
│   └── fixtures/              # audio clips (wav) + expected transcripts
├── docs/                      # this document, ADRs, prompt library
└── .github/workflows/
```

Rule: `apps/desktop` contains **no logic** — only wiring. Every crate compiles and tests headless on CI (audio/inject behind trait fakes), which is what makes the Fable 5 incremental plan verifiable.

---

## 14. Roadmap

| Phase | Scope | Exit criteria | Difficulty | Est. (1 experienced dev, or Fable 5-assisted in ~⅓) |
|---|---|---|---|---|
| **1. Basic dictation** | Tray app, hold-hotkey, cpal capture, VAD trim, whisper.cpp batch transcribe, clipboard-paste inject, minimal settings (mic, hotkey, model dl) | Dictate into Notepad/Chrome/Slack on Win; <2 s for 10 s utterance | Medium | 2–3 wks |
| **2. Streaming** | Ring buffer + pre-roll, Parakeet ONNX engine, partials + HUD overlay, final-pass architecture, latency spans | Partials <300 ms behind speech; release→text <800 ms; HUD ships | Medium-High | 2–3 wks |
| **3. LLM cleanup** | Ollama client, hot-path prompt, fillers/punctuation/paragraphs/lists/corrections, output guard, skip-path, dictionary + snippets + history UI | Messy dictation → clean text; guard catches LLM misbehavior; ≤500 ms added p50 | Medium | 2 wks |
| **4. Context awareness** | Focus/app detection, profiles + auto-switch, UIA/AX nearby-text, entity bias, per-app tone prompts | Slack casual vs email formal demonstrably different; names on screen transcribed right | High (platform APIs) | 2–3 wks |
| **5. Developer mode** | Casing engine, path/symbol/terminal grammars, coding profile prompts, IDE detection, optional qwen3-coder routing | "get user by ID camel case" → `getUserById` in VS Code; terminal-safe injection | Medium | 1–2 wks |
| **6. Voice commands** | Intent gate tiers, structured-output parsing, command registry + custom commands UI, selection transforms, popover assistant, undo | 10 built-in commands work on selections; custom command creatable in UI | Medium | 1–2 wks |
| **7. Performance** | Prompt-prefix caching, prewarm, Vulkan/Metal/CUDA matrix, quant ladder, sentence-streaming injection (opt), p95 hardening | ≤500 ms p50 / ≤800 ms p95 on RTX 3060; CPU-only ≤1.5 s p50 | High | 2 wks |
| **8. Polish** | macOS + Linux injection/context parity, installers + signing + autoupdate, onboarding wizard, themes, docs, plugin API v1 | Cross-platform release on GitHub; new user → first dictation <3 min | Medium-High | 3–4 wks |

Total: **~4–5 months** solo traditional, realistically **6–8 weeks** with Fable 5 doing implementation against the prompt plan. Phases 1–3 alone already beat most commercial offline dictation tools.

---

## 15. Fable 5 Plan

The complete incremental prompt sequence (30 prompts, each with Goal / Files / Architecture / Acceptance criteria / Tests / Refactoring notes) lives in **[FABLE5_PROMPT_PLAN.md](FABLE5_PROMPT_PLAN.md)**.

---

## Appendix A — Sources

- [Wispr Flow: Technical challenges and breakthroughs behind Flow](https://wisprflow.ai/post/technical-challenges) (primary source: latency budget, context-conditioned ASR, token-level formatting, correction learning)
- [Wispr Flow Help Center: Context Awareness](https://docs.wisprflow.ai/articles/4678293671-feature-context-awareness), [Command Mode](https://docs.wisprflow.ai/articles/4816967992-how-to-use-command-mode), [Setup Guide](https://docs.wisprflow.ai/articles/3152211871-setup-guide), [Features](https://wisprflow.ai/features)
- [Weesper: Does Wispr Flow work offline?](https://weesperneonflow.ai/en/blog/2026-02-09-wispr-flow-review-cloud-dictation-2026/) · [Voibe review](https://www.getvoibe.com/resources/wispr-flow-review/) · [Spokenly review](https://spokenly.app/blog/wispr-flow-review) (cloud architecture, RAM/CPU measurements, Electron on Windows)
- [Parakeet TDT vs Whisper (snailtext)](https://snailtext.app/blog/whisper-vs-parakeet-tdt/) · [Northflank open-source STT benchmarks 2026](https://northflank.com/blog/best-open-source-speech-to-text-stt-model-in-2026-benchmarks) · [modelslab parakeet.cpp comparison](https://modelslab.com/blog/audio-generation/parakeet-cpp-vs-whisper-self-hosted-asr-comparison-2026)
- [Moonshine v2 paper](https://arxiv.org/pdf/2602.12241) · [SenseVoice repo](https://github.com/FunAudioLLM/SenseVoice) · [ufal/whisper_streaming (LocalAgreement)](https://github.com/ufal/whisper_streaming) · [WhisperLiveKit](https://github.com/QuentinFuxa/WhisperLiveKit)
- [Handy (Rust/Tauri STT app)](https://deepwiki.com/cjpais/Handy) · [Handy on HN](https://news.ycombinator.com/item?id=45399106) · [OpenWhispr](https://github.com/OpenWhispr/openwhispr) · [keyless](https://github.com/hate/keyless) · [freeflow](https://github.com/zachlatta/freeflow) (prior art)
- [Morph: Best Ollama models June 2026](https://www.morphllm.com/best-ollama-models) · [PromptQuorum Ollama June 2026](https://www.promptquorum.com/local-llms/top-open-source-models-ollama) · [Local AI Master model rankings](https://localaimaster.com/blog/best-ollama-models)
- [AutoHotkey forums: SendInput reliability](https://www.autohotkey.com/boards/viewtopic.php?t=84759) · [clipboard restore pitfalls](https://tdalon.blogspot.com/2021/04/ahk-paste-restore-clipboard-pitfall.html)
- [Tauri global-hotkey](https://github.com/tauri-apps/global-hotkey) · [Tauri v2 global shortcut plugin](https://v2.tauri.app/plugin/global-shortcut/)
