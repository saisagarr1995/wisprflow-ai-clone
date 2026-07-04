# OpenFlow (working title)

An open-source, **fully local** Wispr Flow alternative: push-to-talk dictation into any application, with streaming speech-to-text and LLM-powered cleanup running entirely on your machine via Ollama. No cloud, no telemetry.

**Status:** in development — Phase 1 (basic dictation) underway. Workspace scaffold (Prompt 1) complete: Cargo workspace with 11 library crates + Tauri 2 shell (tray icon, React/Tailwind settings window) + CI.

## Building

```sh
# Prereqs: Rust stable (1.85+), Node 22+, and on first run: npm install in apps/desktop/ui
cd apps/desktop/ui && npm install
npm run tauri dev     # run the desktop app (from apps/desktop/ui)
cargo test --workspace  # run all Rust tests (from repo root)
```

## Documents

- **[docs/ENGINEERING_DESIGN.md](docs/ENGINEERING_DESIGN.md)** — the full engineering design: Wispr Flow reverse-engineering research (known vs. inferred), the complete pipeline stage-by-stage, local model selection (STT + Ollama LLMs), system architecture, context awareness, typing injection, coding mode, voice commands, performance plan, tech stack, project structure, and roadmap.
- **[docs/FABLE5_PROMPT_PLAN.md](docs/FABLE5_PROMPT_PLAN.md)** — 30 incremental Fable 5 (Claude Code) prompts that build the application phase by phase, each with goal, files, architecture, acceptance criteria, tests, and refactoring notes.

## Headline decisions

- **Stack:** Tauri 2 — Rust core (audio, VAD, STT, injection, hotkeys) + React/TS settings UI and HUD overlay.
- **STT:** NVIDIA Parakeet TDT 0.6B v3 (ONNX) for English — natively streaming, faster and more accurate than Whisper large-v3; whisper.cpp `large-v3-turbo` for the other ~99 languages.
- **LLM (Ollama):** small resident model (`qwen3:4b-instruct`) for the hot-path cleanup call; larger models (`gemma3:12b` / `gpt-oss:20b` / `qwen3-coder:30b`) routed per task for rewrites, commands, and coding mode.
- **Latency target:** ≤500 ms p50 from hotkey release to injected text.
