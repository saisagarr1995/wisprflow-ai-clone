# OpenFlow architecture reference

Deep-dive context for working in this repo. See `docs/ENGINEERING_DESIGN.md` for
the full design and `.claude/PROGRESS.md` for phase status.

## Workspace map

| Crate | Responsibility |
|-------|----------------|
| `core` | `SessionOrchestrator` — single-threaded mpsc event loop, the only owner of session state. Wires every service together. |
| `audio` | cpal capture → resample to 16 kHz mono → rolling buffer w/ pre-roll; Silero VAD (v5 needs a 64-sample context prefix — `[1,576]` input). |
| `stt` | `SttEngine` trait; whisper.cpp backend (whisper-rs 0.16). |
| `llm` | `LlmBackend` trait; Ollama client; cleanup prompt + output guard; command registry + intent gate; task→model router. |
| `postproc` | dictionary fuzzy-replace, snippet triggers, coding-mode casing/grammar. Pure functions. |
| `context` | focused-app identity (Win32), profile classifier, tone blocks, UIA nearby-text capture, entity extraction. |
| `inject` | `Injector` trait; clipboard-paste + keystroke (terminal-safe, never sends newline). |
| `hotkey` | low-level `WH_KEYBOARD_LL` hook + pure `ChordTracker`. |
| `settings` | typed config, atomic save, versioned migrations, hot-reload watch channel. |
| `store` | SQLite: dictionary, snippets, history, custom commands. |
| `models` | model registry (checksummed), resumable downloads, Ollama presence check. |
| `apps/desktop` | Tauri shell: tray, windows (settings + HUD + assistant popover), IPC. **No logic.** |

## Load-bearing invariants

- The orchestrator is single-threaded; all state transitions go through its
  channel. Never add a second owner of session state.
- Hardware/platform code sits behind a trait with a `Fake*` impl so tests run
  headless (CI has no mic/GPU/Ollama).
- LLM output is never trusted: `llm::guard::sanitize` strips/validates and the
  pipeline falls back to the raw transcript on any failure or guard trip.
- Terminal injection strips every newline (a newline would execute commands).
- Config changes flow through the watch channel; no crate reads the file
  directly except `settings`.

## Where things live at runtime (Windows)

- Config: `%APPDATA%\openflow\config.json`
- Models (whisper/vad): `%APPDATA%\openflow\models\`
- Database: `%APPDATA%\openflow\openflow.db`
- LLM models: managed by Ollama (`ollama list`)

## Deferred / roadmap

Parakeet STT engine, macOS/Linux platform backends (traits + fakes ready),
onboarding wizard, installer code-signing + auto-updater (need signing keys).
