# OpenFlow

A fully local, open-source **Wispr Flow alternative**: push-to-talk voice dictation into any application, with on-device speech recognition and local LLM cleanup via Ollama. No cloud, no telemetry, no subscription.

Hold a hotkey, speak, release — polished text appears at your cursor in whatever app is focused.

![status](https://img.shields.io/badge/status-all%20phases%20implemented-success)

## What it does

- **Global push-to-talk** — hold Right Ctrl (configurable; hold or toggle mode) to dictate into any app.
- **On-device speech-to-text** — whisper.cpp locally; live streaming partials in a floating HUD.
- **Local AI cleanup (Ollama)** — removes filler words ("um", "uh"), fixes punctuation, applies self-corrections ("no wait, Monday"), formats paragraphs and lists. Falls back to the raw transcript whenever the model misbehaves or Ollama is down.
- **Context awareness** — detects the focused app (Slack, Gmail, VS Code, terminal, docs) and adapts tone; reads nearby text to spell names right; aborts if you switch windows mid-dictation.
- **Coding mode** — "get user by id camel case" → `getUserById`; "src slash main dot rs" → `src/main.rs`; "git commit dash m" → `git commit -m`. Types (never pastes) into terminals and never sends a newline.
- **Voice commands** — select text and say "make it shorter", "rewrite professionally", "translate to German", "turn this into an email", "summarize", "fix grammar", or "explain this" (answer shown in a popover). Add your own.
- **Personal dictionary** — teach it your jargon and names; corrects mis-hearings and biases the recognizer.
- **Snippets** — say "my address" → your address expands.
- **History & latency monitor** — every dictation logged locally with a per-stage timing waterfall.
- **100% local** — the only network calls are to your local Ollama and (once) model downloads.

## Architecture

Tauri 2 desktop app: a Rust core (the tray-resident daemon) plus a React/TypeScript settings window and a transparent HUD overlay.

```
Mic → VAD → rolling buffer ──► streaming partials (HUD)
                    │ (hotkey release)
                    ▼
        Whisper final pass → dictionary → coding grammar → snippets
                    │
              command? ──yes──► LLM transform → replace selection / popover
                    │no
                    ▼
        LLM cleanup (Ollama, guarded) → injection (clipboard paste / keystrokes)
                    ▼
             your focused application
```

Crates: `core` (orchestrator state machine), `audio` (cpal + Silero VAD), `stt` (whisper.cpp), `llm` (Ollama client + cleanup + commands + guard), `postproc` (dictionary, snippets, casing), `context` (focus + UIA + profiles), `inject` (clipboard/keystroke), `hotkey` (low-level hook), `settings`, `store` (SQLite), `models` (downloads). Full design in [docs/ENGINEERING_DESIGN.md](docs/ENGINEERING_DESIGN.md); the incremental build plan in [docs/FABLE5_PROMPT_PLAN.md](docs/FABLE5_PROMPT_PLAN.md); progress in [.claude/PROGRESS.md](.claude/PROGRESS.md).

## Building & running (Windows)

Prereqs: **Rust** (stable, MSVC toolchain), **Node 22+**, **CMake** and **LLVM/libclang** (whisper.cpp builds via cmake + bindgen), and **[Ollama](https://ollama.com)** for AI cleanup.

```sh
# One-time: pull the default local models
ollama pull qwen3:4b-instruct
ollama pull gemma3:12b

cd apps/desktop
npm install
npm run tauri dev      # run the app (dev)
npm run tauri build    # produce an installer (NSIS .exe / .msi)
```

From the repo root: `cargo test --workspace` runs the suite; `cargo clippy --workspace --all-targets -- -D warnings` lints.

First launch downloads the speech model and Silero VAD automatically, then lives in the system tray. Open **Settings** from the tray to pick your mic, model, hotkey, and to manage your dictionary, snippets, and commands.

> **Note on scope:** the Windows implementation is complete. macOS and Linux backends (hotkey/injection/context) are stubbed behind the same traits and are the next port — the core, audio, STT, LLM, and UI are already cross-platform.

## Privacy

There is no analytics or telemetry code, and no account. The app connects only to your local Ollama (`http://localhost:11434` by default) and to Hugging Face/GitHub once to download the speech models you choose. Everything else — audio, transcripts, history — stays on your machine.

## License

MIT — see [LICENSE](LICENSE).
