# OpenFlow

A fully local, open-source **Wispr Flow alternative**: push-to-talk voice dictation into any application, with on-device speech recognition and optional local LLM cleanup via Ollama. No cloud, no telemetry, no account, no subscription.

Hold a hotkey, speak, release — polished text appears at your cursor in whatever app is focused.

![status](https://img.shields.io/badge/status-all%20phases%20implemented-success)
![platform](https://img.shields.io/badge/platform-Windows%2010%2F11-blue)
![license](https://img.shields.io/badge/license-MIT-green)

---

## Table of contents

1. [Features](#features)
2. [How it works (architecture)](#how-it-works-architecture)
3. [System requirements](#system-requirements)
4. [Installation — Option A: prebuilt installer (recommended)](#installation--option-a-prebuilt-installer-recommended)
5. [Installation — Option B: build from source](#installation--option-b-build-from-source)
6. [First-run setup](#first-run-setup)
7. [Dictating in Tamil, Telugu, and other languages](#dictating-in-tamil-telugu-and-other-languages)
8. [Optional: AI cleanup with Ollama](#optional-ai-cleanup-with-ollama)
9. [Using OpenFlow](#using-openflow)
10. [Configuration reference](#configuration-reference)
11. [Troubleshooting](#troubleshooting)
12. [Development](#development)
13. [Privacy](#privacy)
14. [License](#license)

---

## Features

- **Global push-to-talk** — hold a hotkey (e.g. **Ctrl + Win** or Right Ctrl) to dictate into any app. Three modes: **Hold** (push-to-talk), **Toggle** (press to start/stop), and **Hands-free** (press once; it stops by itself when you go quiet).
- **Clickable floating pill** — hover the little bar at the bottom of the screen: *"Click or hold Ctrl + Win to start dictating"*. Click it to start; while recording, **✕** cancels and the red **stop** button finishes and pastes — no keyboard needed.
- **Languages** — English plus Tamil, Telugu, Hindi and ~95 more via multilingual Whisper models, with automatic language detection.
- **On-device speech-to-text** — whisper.cpp runs locally; live streaming captions in a floating HUD while you speak; long dictations finalize in flat time thanks to streaming chunk finalization.
- **Instant deterministic cleanup (no LLM required)** — repeated words/stutters collapsed ("the the" → "the"), fillers removed ("um", "uh"), sentences capitalized, terminal punctuation added with question detection ("what time is it" → "What time is it?"). Runs in microseconds on every dictation.
- **Optional local AI cleanup (Ollama)** — deeper rewrites: self-corrections ("no wait, Monday"), paragraph/list formatting, tone matching. Guarded — falls back to the raw transcript whenever the model misbehaves or Ollama is down.
- **Context awareness** — detects the focused app (Slack, Gmail, VS Code, terminal, docs) and adapts tone; reads nearby text to spell names right; aborts injection if you switch windows mid-dictation.
- **Coding mode** — "get user by id camel case" → `getUserById`; "src slash main dot rs" → `src/main.rs`. Types (never pastes) into terminals and never sends a newline.
- **Voice commands** — select text and say "make it shorter", "rewrite professionally", "translate to German", "summarize", "fix grammar", or "explain this" (answer in a popover). Add your own.
- **Personal dictionary** — teach it your jargon and names; corrects mis-hearings and biases the recognizer.
- **Snippets** — say "my address" → your full address expands.
- **History & latency monitor** — every dictation logged locally (SQLite) with a per-stage timing waterfall.
- **100% local** — the only network calls are to your local Ollama and one-time model downloads.

## How it works (architecture)

Tauri 2 desktop app: a Rust core (tray-resident daemon) plus a React/TypeScript settings window and a transparent HUD overlay.

```
Mic → VAD → rolling buffer ──► streaming captions (HUD)
                    │            + chunk finalization while you speak
                    │ (hotkey release: only the last few seconds decode)
                    ▼
   Whisper final pass → dictionary → coding grammar / prose tidy → snippets
                    │
              command? ──yes──► LLM transform → replace selection / popover
                    │no
                    ▼
   LLM cleanup (Ollama, optional, guarded) → injection (clipboard paste / keystrokes)
                    ▼
             your focused application
```

Crates: `core` (orchestrator state machine), `audio` (cpal + Silero VAD), `stt` (whisper.cpp), `llm` (Ollama client + cleanup + commands + guard), `postproc` (tidy, dictionary, snippets, casing), `context` (focus + UIA + profiles), `inject` (clipboard/keystroke), `hotkey` (low-level hook), `settings`, `store` (SQLite), `models` (downloads). Full design in [docs/ENGINEERING_DESIGN.md](docs/ENGINEERING_DESIGN.md); build plan in [docs/FABLE5_PROMPT_PLAN.md](docs/FABLE5_PROMPT_PLAN.md); progress in [.claude/PROGRESS.md](.claude/PROGRESS.md).

## System requirements

| | Minimum | Recommended |
|---|---|---|
| OS | Windows 10 64-bit | Windows 11 |
| CPU | 4 cores | 8+ cores |
| RAM | 8 GB (dictation only) | 16 GB (dictation + Ollama cleanup) |
| Disk | ~500 MB (app + whisper-base + VAD) | +2 GB per Ollama model |
| Other | A microphone; [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Win 10/11) | |

> **Low-RAM machines (8 GB or less):** dictation works well, but keep LLM cleanup disabled (the default deterministic tidy still gives you clean text) and don't let Ollama autostart in the background — see [Troubleshooting](#troubleshooting).

macOS/Linux: the core crates are cross-platform and CI-built, but the hotkey/injection/context backends are Windows-only today; ports are planned behind the same traits.

## Installation — Option A: prebuilt installer (recommended)

**No account, no build tools, nothing to compile — download one file and run it.**

1. Go to the repo's **[Releases page](../../releases/latest)** (right sidebar on GitHub → *Releases*).
2. Under **Assets**, download **`OpenFlow_<version>_x64-setup.exe`** (~14 MB).
3. Double-click it. It installs per-user to `%LOCALAPPDATA%\OpenFlow` — **no admin rights needed**. (Windows SmartScreen may warn because the exe is unsigned; choose *More info → Run anyway*.) Silent install: `OpenFlow_<version>_x64-setup.exe /S`
4. Launch **OpenFlow** from the Start menu. It lives in the system tray (no main window by design).
5. On first dictation the speech model (~150 MB) downloads automatically to `%APPDATA%\openflow\models`.
6. Continue with [First-run setup](#first-run-setup).

The installer runs on any 64-bit Windows 10/11 machine (built against the standard AVX2 CPU baseline — any Intel/AMD CPU from ~2013 on). It ships CPU transcription by default; machines with a discrete GPU can flip **Settings → Models & AI → GPU transcription**.

To upgrade, install the newer version over the old one. To remove, use Windows **Add/Remove Programs → OpenFlow**.

<details>
<summary>Alternative: download a development build from GitHub Actions (needs a GitHub login)</summary>

Every push to `main` also uploads artifacts: repo **Actions** tab → workflow **"Build Windows App"** → latest green run → download **`OpenFlow-Windows-Installer`** (or `OpenFlow-Windows-Standalone` for a portable exe) and unzip. With the [GitHub CLI](https://cli.github.com/): `gh run download <run-id> -n OpenFlow-Windows-Installer`
</details>

## Installation — Option B: build from source

### Step 1 — install the toolchain

Run these in **PowerShell** (winget comes with Windows 11; on Windows 10 install [App Installer](https://apps.microsoft.com/detail/9NBLGGH4NNS1)):

```powershell
# Rust (MSVC toolchain — the default on Windows)
winget install Rustlang.Rustup
rustup default stable-msvc

# Visual Studio 2022 Build Tools with the C++ workload (MSVC linker, Windows SDK)
winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"

# Node.js 22+ (frontend build)
winget install OpenJS.NodeJS.LTS

# CMake + LLVM (whisper.cpp builds via cmake; bindgen needs libclang)
winget install Kitware.CMake
winget install LLVM.LLVM
```

Open a **new** PowerShell window afterwards so `PATH` changes take effect. Verify:

```powershell
rustc --version      # e.g. rustc 1.8x
node --version       # v22+
cmake --version
clang --version
```

> If `cargo build` later fails with a *libclang not found* error, point bindgen at LLVM explicitly by creating `.cargo/config.toml` in the repo root:
>
> ```toml
> [env]
> LIBCLANG_PATH = "C:\\Program Files\\LLVM\\bin"
> ```

### Step 2 — clone and build the frontend once

```powershell
git clone https://github.com/saisagarr1995/wisprflow-ai-clone.git
cd wisprflow-ai-clone\apps\desktop
npm install
npm run build        # creates apps/desktop/dist — required before any cargo build
```

### Step 3 — build and run

```powershell
# Development (hot-reload UI + debug Rust core):
npm run tauri dev

# Release installer (NSIS .exe under target/release/bundle/):
npm run tauri build
```

Build from **PowerShell, not Git Bash** — Git Bash can shadow the MSVC linker with coreutils `link` and fail with `link: extra operand`.

> **Tip for 8 GB machines:** a full release build is memory-hungry. Close browsers/Ollama first, or limit parallelism: `$env:CARGO_BUILD_JOBS=2; npm run tauri build`. Alternatively just use the CI installer (Option A).

### Step 4 — verify the workspace (optional, for contributors)

```powershell
cd ..\..            # repo root
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## First-run setup

1. **Launch OpenFlow.** On first start it automatically downloads the speech model (whisper-base, ~148 MB) and Silero VAD (~2 MB) to `%APPDATA%\openflow\models`, then sits in the system tray.
2. **Open Settings** from the tray icon.
3. **Microphone** — pick your input device under *Audio* (default: system default device).
4. **Hotkey** — default is **hold Right Ctrl** (pick **Ctrl+Win** or others from the presets). Press and *hold* it, speak, release. (A quick tap is deliberately ignored as a fumble — utterances under 300 ms are discarded.) Prefer no keyboard? Choose **Toggle** or **Hands-free** mode in Settings → General, or simply **click the floating pill** at the bottom of the screen to start and use its ✕ / stop buttons to cancel or finish.
5. **Test it:** focus Notepad, hold Right Ctrl, say "this is a test of open flow", release. Text appears at the cursor and the floating HUD shows live captions while you speak.
6. (Optional) Set up [AI cleanup](#optional-ai-cleanup-with-ollama), your dictionary, and snippets.

## Dictating in Tamil, Telugu, and other languages

The default `whisper-base` model is **English-only**. For any other language (or auto-detection across languages) you need a **multilingual** model — three are built in:

1. Open **Settings → Models & AI** and download one:
   - `whisper-small-multi` (~490 MB) — **recommended for Tamil/Telugu**; noticeably better accuracy for Indian languages.
   - `whisper-base-multi` (~148 MB) — faster, okay accuracy.
2. Still in **Models & AI**, select it as the active speech model.
3. In **Settings → General → Language**, pick **Tamil**, **Telugu**, or leave **Auto-detect** (auto works across languages; forcing one language is a bit faster and more reliable).
4. Dictate as usual — captions and the final text come out in the spoken language and are typed into the focused app.

> Tips: on slower machines `whisper-small-multi` takes longer per dictation than base — try both. Keep **AI cleanup off** for non-English text unless your Ollama model handles that language well; the built-in deterministic tidy is script-safe.

## Optional: AI cleanup with Ollama

Dictation is fully functional without this — the built-in deterministic tidy already removes stutters/fillers and fixes capitalization/punctuation. Enable Ollama cleanup for deeper rewrites (self-corrections, formatting, tone).

1. Install Ollama: `winget install Ollama.Ollama` (or from [ollama.com](https://ollama.com)).
2. Pull a small, fast model — on CPU-only machines stick to 1–3 B parameters:
   ```powershell
   ollama pull llama3.2:1b     # fastest, ~1.2 GB
   # or: ollama pull llama3.2:3b   # better quality, needs more RAM/CPU
   ```
3. In OpenFlow **Settings → Models & AI**: enable cleanup and select the model. The status card turns green when Ollama is reachable at `http://localhost:11434`.
4. If Ollama is down or slow, dictation still works — cleanup silently falls back to the tidied raw transcript.

> **8 GB-RAM warning:** Ollama's tray app registers itself to autostart and holds RAM even when idle, which can starve the speech decoder. If you don't use cleanup, remove the autostart:
> ```powershell
> Remove-ItemProperty -Path HKCU:\Software\Microsoft\Windows\CurrentVersion\Run -Name Ollama
> ```

## Using OpenFlow

| Action | How |
|---|---|
| Dictate (keyboard) | Hold your hotkey (e.g. **Ctrl + Win**), speak, release |
| Dictate (mouse) | Click the little pill at the bottom of the screen; while recording click **✕** to cancel or the red **stop** to finish and paste |
| Hands-free | Settings → General → Mode → **Hands-free**: press the hotkey (or click the pill) once, speak, and just stop talking — it finishes on its own after ~2 s of silence |
| Live captions | Appear in the floating HUD pill while recording |
| Voice command | Select text first, then dictate e.g. *"make it shorter"* |
| Ask a question | Dictate *"explain this"* with text selected — answer opens in a popover |
| Snippet | Dictate its trigger phrase, e.g. *"my address"* |
| Coding mode | Automatic in IDEs/terminals: *"get user by id camel case"* → `getUserById` |
| History / latency | Settings → History / Latency pages |

## Configuration reference

Config lives at `%APPDATA%\openflow\config.json` — edit through the Settings UI. (If you must edit by hand, save as UTF-8 **without BOM**; a BOM makes the file unreadable and the app resets it to defaults.)

| Setting | Default | Notes |
|---|---|---|
| `hotkey.chord` / `mode` | `RCtrl` / `hold` | Presets incl. `Ctrl+Win`; modes: `hold`, `toggle`, `hands_free` (auto-stops ~2 s after you stop talking) |
| `stt.model_id` | `whisper-base` | English-only; use `whisper-small-multi` / `whisper-base-multi` for Tamil/Telugu/auto-detect |
| `language` | `auto` | Or force `en`, `ta`, `te`, `hi`, … (a bit faster than auto) |
| `llm.enabled` | `false` | Ollama cleanup on/off |
| `llm.cleanup_model` | `llama3.2:1b` | Any local Ollama model |
| `audio.preroll_ms` | `250` | Audio kept from before the keypress |
| `injection.strategy` | `auto` | Clipboard-paste, keystrokes for terminals |

Data locations: models `%APPDATA%\openflow\models`, history DB `%APPDATA%\openflow\openflow.db`, logs to stdout (`RUST_LOG=openflow_core=debug` for verbose).

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Nothing happens on Right Ctrl | App not running (check tray / `Get-Process openflow-desktop`); or you *tapped* instead of *held* — sub-300 ms utterances are discarded |
| Everything is slow (10–20 s finals) | RAM starvation. Check free RAM; quit Ollama's tray app and remove its autostart (command above); close heavy browsers. Dictation needs ~500 MB free |
| First dictation after launch is slower | Model warm-up; engines are pre-warmed at startup, but a cold disk cache can still add a second once |
| Text lands in the wrong app | It doesn't — injection is aborted if focus changed mid-dictation; re-focus and dictate again |
| Cleanup returns raw text | Ollama down/unreachable → check Settings status card; the guard also rejects LLM output that invents content |
| `link: extra operand` build error | Building from Git Bash — use PowerShell |
| Build fails: libclang / cmake missing | Install LLVM + CMake (Step 1) and/or set `LIBCLANG_PATH` |
| `generate_context!` error on cargo build | Run `npm run build` in `apps/desktop` first (dist must exist) |
| Config reset to defaults on start | Hand-edited config saved with a UTF-8 BOM — recreate via Settings UI |
| HUD never reacts | Reinstall — a build without `capabilities/default.json` denies HUD events |

Per-dictation stage timings (audio/STT/LLM/inject) are recorded in **Settings → Latency** — check them first; they name the slow stage.

## Development

```powershell
cargo test --workspace                                   # full test suite (headless; hardware is faked)
cargo clippy --workspace --all-targets -- -D warnings    # lint gate
cd apps\desktop; npm run tauri dev                       # run with hot reload
```

- `apps/desktop` is a thin Tauri shell (tray, windows, IPC) — **all logic lives in `crates/*`**.
- Hardware/platform code sits behind traits with `Fake*` implementations so tests run headless in CI.
- The orchestrator (`crates/core`) is a single-threaded mpsc event loop — the only owner of session state.
- PRs must pass `cargo test`, clippy with `-D warnings`, and `cargo fmt --check`.

## Privacy

There is no analytics or telemetry code, and no account. The app connects only to your local Ollama (`http://localhost:11434` by default) and to Hugging Face/GitHub once to download the speech models you choose. Everything else — audio, transcripts, history — stays on your machine.

## License

MIT — see [LICENSE](LICENSE).
