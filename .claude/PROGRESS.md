# Build Progress — Fable 5 Prompt Plan

Tracks position in [docs/FABLE5_PROMPT_PLAN.md](../docs/FABLE5_PROMPT_PLAN.md). Update the status column as prompts complete; add notes for deviations from the plan.

| # | Prompt | Status | Notes |
|---|--------|--------|-------|
| 1 | Workspace scaffold | ✅ done | 11 crates + Tauri shell + CI + icons. Verified pending first full linked build (MSVC was missing on dev machine, installed 2026-07-04). |
| 2 | Settings manager | 🔨 in progress | Code + tests written; verification pending MSVC install. |
| 3 | Audio capture engine | ⬜ | |
| 4 | VAD service | ⬜ | |
| 5 | STT trait + whisper.cpp | ⬜ | |
| 6 | Hotkey listener | ⬜ | |
| 7 | Model manager | ⬜ | |
| 8 | Orchestrator v1 + injection | ⬜ | Phase 1 demo: dictate into Notepad/Chrome/Slack |
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
