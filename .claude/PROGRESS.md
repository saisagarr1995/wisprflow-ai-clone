# Build Progress — Fable 5 Prompt Plan

Tracks position in [docs/FABLE5_PROMPT_PLAN.md](../docs/FABLE5_PROMPT_PLAN.md). Update the status column as prompts complete; add notes for deviations from the plan.

| # | Prompt | Status | Notes |
|---|--------|--------|-------|
| 1 | Workspace scaffold | ✅ done | 11 crates + Tauri shell + CI + icons. Layout deviation: frontend at `apps/desktop` (standard Tauri), not `apps/desktop/ui` — the CLI requires it. |
| 2 | Settings manager | ✅ done | 8 tests. Hot-reload uses read-only `try_load` + retry (never "repairs" a mid-write file). |
| 3 | Audio capture engine | ✅ done | Rolling buffer w/ absolute indices instead of raw SPSC for sessions; cpal callback → rtrb → drain thread. |
| 4 | VAD service | ✅ done | Silero v5 needs 64-sample context prefix ([1,576] input) — bare [1,512] silently yields garbage. Model + jfk.wav committed. |
| 5 | STT trait + whisper.cpp | ✅ done | whisper-rs 0.16 (0.14 has broken Windows bindings). Real-model test: JFK transcribes perfectly w/ tiny.en. Needs cmake + LLVM (libclang) to build. |
| 6 | Hotkey listener | ✅ done | Pure ChordTracker + WH_KEYBOARD_LL thread; hold/toggle decided in orchestrator, not listener. |
| 7 | Model manager | ✅ done | tiny/base/silero in registry w/ real checksums; turbo+parakeet deferred to Prompt 10. |
| 8 | Orchestrator v1 + injection | ✅ done | Working dictation on Windows. Trailing-space chaining, <300ms fumble gate, VAD trim. Manual multi-app checklist: user to verify. |
| 9 | Settings UI v1 | ⬜ | |
| 10 | Parakeet ONNX engine | ⬜ | |
| 11 | Streaming partials + two-pass | ⬜ | |
| 12 | HUD overlay + latency spans | ⬜ | |
| 13 | Ollama client + router | ⬜ | |
| 14 | Prompt pipeline + output guard | ⬜ | |
| 15 | Personal dictionary | ⬜ | |
| 16 | Snippets | ⬜ | |
| 17 | History + logs UI | ⬜ | |
| 18 | Focus & app identity | ⬜ | |
| 19 | Nearby text via UIA | ⬜ | |
| 20 | Profiles + auto-switch | ⬜ | |
| 21 | Casing engine + code grammars | ⬜ | |
| 22 | Coding profile + IDE awareness | ⬜ | |
| 23 | Intent gate + command parsing | ⬜ | |
| 24 | Command registry + transforms | ⬜ | |
| 25 | Custom commands UI + popover | ⬜ | |
| 26 | Latency hardening | ⬜ | |
| 27 | Latency monitor UI + progressive injection | ⬜ | |
| 28 | macOS parity | ⬜ | |
| 29 | Linux parity | ⬜ | |
| 30 | Release engineering + onboarding | ⬜ | |

## Machine notes
- Dev machine: Windows 11, Rust stable (MSVC), Node 22. VS Build Tools 2022 installed 2026-07-04.
- Build from PowerShell, not Git Bash (Git Bash's coreutils `link` shadows the MSVC linker).
- `apps/desktop/ui`: `npm run build` must run once before the first `cargo build` (Tauri `generate_context!` needs `dist/`).
