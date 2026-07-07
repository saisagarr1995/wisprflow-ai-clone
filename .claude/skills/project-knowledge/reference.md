# Project knowledge — deep-dive reference

Stable facts that don't change per-file (the per-file map is `PROJECT_TREE.md`).

## Architecture in one paragraph

OpenFlow is a fully local push-to-talk dictation app. Tauri 2 shell
(`apps/desktop`, React/TS UI + thin Rust IPC layer, **no logic**) over a
workspace of crates. `crates/core` owns all session state in a
single-threaded mpsc event loop (the orchestrator). Audio capture →
VAD → STT (whisper.cpp, Parakeet planned) → post-processing → LLM cleanup
(Ollama) → text injection into the focused app. Hardware/platform code sits
behind traits with `Fake*` impls so tests run headless.

## Where things go

- New business logic → the matching `crates/*`, never `apps/desktop`.
- New shared dependency versions → `[workspace.dependencies]` in root `Cargo.toml`.
- New IPC surface → `apps/desktop/src-tauri/src/commands.rs` (thin wrapper only).

## Source-of-truth docs

- `docs/ENGINEERING_DESIGN.md` — architecture & pipeline; § numbers referenced everywhere.
- `docs/FABLE5_PROMPT_PLAN.md` — 30-prompt build plan; current position in `.claude/PROGRESS.md` / README "Status".

## Build environment (Windows quirks)

- Build from **PowerShell**, not Git Bash (Git Bash's `link` shadows the MSVC linker).
- whisper.cpp needs CMake + LLVM/libclang; `.cargo/config.toml` (gitignored) sets
  `LIBCLANG_PATH`/`CMAKE`. If missing-tool errors appear, use `scripts/build.ps1`
  (see the `build-and-run` skill).
- `npm run build` in `apps/desktop` must run once before the first cargo build
  (`generate_context!` needs `dist/`).

## Definition of done (every change)

`cargo test --workspace` green · `cargo clippy --workspace --all-targets -- -D warnings` clean · prompt acceptance criteria verified (manually where hardware is involved).
