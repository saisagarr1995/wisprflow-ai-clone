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
| 9 | Settings UI v1 | ✅ done | React settings: General/Microphone/Models pages, live mic meter, model download manager, Ollama status card. Frontend at `apps/desktop`. |
| 10 | Parakeet ONNX engine | ⏭ deferred | whisper.cpp covers STT; Parakeet is a perf/accuracy upgrade for a later pass. Registry + engine-abstraction ready for it. |
| 11 | Streaming partials + two-pass | ✅ done | Chunked re-decode (whisper tiny) on a dedicated PartialWorker thread; final pass authoritative. LocalAgreement deferred with Parakeet. |
| 12 | HUD overlay + latency spans | ✅ done | Transparent click-through waveform capsule (matches user's reference image) + per-stage timings emitted to UI. |
| 13 | Ollama client + router | ✅ done | Non-streaming /api/chat, keep_alive -1, prewarm, task→model router. |
| 14 | Prompt pipeline + output guard | ✅ done | Cleanup prompt w/ few-shots; guard strips think-blocks/fences/meta, length-ratio check, raw-text fallback; skip-path. 14 llm tests. |
| 15 | Personal dictionary | ✅ done | SQLite + squashed-token Jaro-Winkler w/ length-ratio guard; STT initial_prompt bias; UI page. |
| 16 | Snippets | ✅ done | Whole-utterance triggers (opt. "insert" prefix), short-circuit LLM; UI page. |
| 17 | History + logs UI | ✅ done | history table (content only, no screen context), retention cap, search, raw↔formatted diff, timings. |
| 18 | Focus & app identity | ✅ done | Win32 foreground process+title, profile classifier (chat/email/coding/terminal/document), tone blocks, focus-change abort. |
| 19 | Nearby text via UIA | ✅ done | UIA TextPattern/ValuePattern w/ 150ms timeout thread; selection capture; entity extraction → STT bias. |
| 20 | Profiles + auto-switch | ✅ done (auto) | App→profile auto-classification drives tone + injection strategy. Explicit profile-editor UI left as roadmap. |
| 21 | Casing engine + code grammars | ✅ done | camel/pascal/snake/kebab/screaming/dot; paths, symbols, terminal flags. Pure fns, table tests. |
| 22 | Coding profile + IDE awareness | ✅ done | Coding tone prompt; terminal → keystroke injector w/ hard newline-strip invariant (never executes commands). IDE-extension socket = roadmap. |
| 23 | Intent gate + command parsing | ✅ done | Tier-0 regex trigger match w/ operand requirement + arg capture (translate lang, ask question). |
| 24 | Command registry + transforms | ✅ done | 11 built-ins on selection/last-injection via Ollama; guarded; undo via last_injection tracking. |
| 25 | Custom commands UI + popover | ✅ done | Commands settings page (CRUD); assistant answers stream to a dedicated popover window, never injected. |
| 26 | Latency hardening | ✅ partial | Prewarm + per-stage spans + skip-paths done. Prompt-prefix cache tuning + `bench` CLI = roadmap. |
| 27 | Latency monitor UI | ✅ done | Live per-stage waterfall + p50/p95 for last 50 dictations. Progressive injection = roadmap. |
| 28 | macOS parity | ⏭ deferred | Traits + fakes in place; mac backends (CGEventTap/AX/NSPasteboard) are the next port. |
| 29 | Linux parity | ⏭ deferred | Same trait boundary; X11/Wayland backends next. |
| 30 | Release engineering | ✅ partial | NSIS/MSI/dmg/appimage/deb bundle config, MIT LICENSE, no-telemetry test, docs. Signing + auto-updater need keys (updater shipped disabled). Onboarding wizard = roadmap. |

## Machine notes
- Dev machine: Windows 11, Rust stable (MSVC), Node 22. VS Build Tools 2022 installed 2026-07-04.
- Build from PowerShell, not Git Bash (Git Bash's coreutils `link` shadows the MSVC linker).
- `apps/desktop/ui`: `npm run build` must run once before the first `cargo build` (Tauri `generate_context!` needs `dist/`).
