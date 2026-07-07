# OpenFlow — local Wispr Flow alternative

Fully local push-to-talk dictation app: Tauri 2 (Rust core + React/TS UI), STT via Parakeet/whisper.cpp, LLM cleanup via Ollama. No cloud, no telemetry.

## Source of truth
- `.claude/skills/project-knowledge/` — the project knowledge base. `tree.md` = auto-generated repo map (**read first to locate code instead of globbing**); `flows.md` = business flows + latency-triage table (**read first for any bug/latency/behavior question, then open only the file it points at**). See its SKILL.md for maintenance rules.
- `docs/ENGINEERING_DESIGN.md` — architecture, pipeline, model choices (§ numbers referenced everywhere).
- `docs/FABLE5_PROMPT_PLAN.md` — the 30-prompt incremental build plan. Work proceeds prompt by prompt, in order. Check README "Status" for the current prompt.

## Layout & rules
- `apps/desktop` is a thin Tauri shell: tray, windows, IPC wiring only. **No logic.**
- All logic lives in `crates/*` (core, audio, stt, llm, postproc, context, inject, hotkey, settings, store, models).
- Hardware/platform code (audio, injection, hooks, UIA) goes behind a trait with a `Fake*` impl so tests run headless in CI.
- The orchestrator in `crates/core` is a single-threaded mpsc event loop — it is the only owner of session state.
- Shared dependency versions live in `[workspace.dependencies]` in the root `Cargo.toml`.

## Commands
- Build/test Rust: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
- UI (frontend lives at `apps/desktop`, standard Tauri layout with `src-tauri/` inside): `npm run dev` / `npm run build`; run app: `npm run tauri dev` (all from `apps/desktop`)
- The desktop crate's `generate_context!` needs `apps/desktop/dist` to exist — run `npm run build` in `apps/desktop` once before the first cargo build.

## Definition of done (every prompt)
`cargo test --workspace` green, clippy clean with `-D warnings`, acceptance criteria of the prompt verified (manually where hardware is involved).
