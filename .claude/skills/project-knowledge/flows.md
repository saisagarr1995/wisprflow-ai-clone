# Business Flows & Latency Map

Hand-maintained companion to the auto-generated `tree.md` (same folder).
Read this INSTEAD of the code when debugging behavior or latency; then open
only the one file the flow points at. **Update this file whenever a flow
changes** (new pipeline stage, changed timing knob, new event).

## Flow 1 — Push-to-talk dictation (the core flow)

Hotkey (default RCtrl; presets incl. Ctrl+Win) held → speak → release → text
appears in focused app. Three hotkey modes (config `hotkey.mode`, applied
live via ConfigChanged): **hold** (release finishes), **toggle** (second
press finishes), **hands_free** (auto-finishes after ~2 s of trailing
silence — level-based endpointing in `Orchestrator::hands_free_should_stop`,
driven by a 200 ms `recv_timeout` tick in `run`; a second press also stops).
The HUD pill can drive the same session with the mouse:
`SessionEvent::{HudStart,HudFinish,HudCancel}` posted from
`commands.rs::hud_start/hud_finish/hud_cancel` (pill click / stop button /
X button). Cancel (`cancel_recording`) discards everything — no STT, no
injection.

```
key event (any key on system)
  → windows_hook.rs low-level keyboard hook          [crates/hotkey/src/windows_hook.rs]
  → ChordTracker::on_key → Pressed/Released           [crates/hotkey/src/tracker.rs]
  → thread in shell maps to SessionEvent::HotkeyPressed/Released   [apps/desktop/src-tauri/src/lib.rs ~line 422]
  → Orchestrator event loop (single thread, owns ALL state)        [crates/core/src/orchestrator.rs]
```

**On press** — `Orchestrator::begin_recording` (orchestrator.rs:222):
1. `audio.begin_session(preroll_ms)` — capture starts from ring buffer with pre-roll.
2. `PartialWorker.start` — live captions begin (separate thread + small model).
3. Context snapshot (focused app, profile, on-screen text) — parallel to speaking.
4. `build_opts` — STT decode options incl. dictionary + entity bias prompt.
5. `StreamFinalizer.start` — begins committing chunks with the FINAL engine while still speaking.

**On release** — `Orchestrator::finish_recording` (orchestrator.rs:254), stages in order:
1. `finalizer.finish()` — collect text already committed during speech; only the tail (last ≤~6s) remains.
2. `audio.end_session()` + VAD `trim_silence`; <300 ms audio = fumble, skipped entirely.
3. **STT tail decode** — `stt.transcribe(tail)` (whisper.cpp, all-but-one cores). → `timing.stt_ms`
4. Voice command check `try_run_command` — if the utterance matches a command, dictation path stops here (Flow 2).
5. Post-processing (deterministic, fast): dictionary fuzzy-replace → coding grammar/casing (Coding/Terminal profile) OR prose tidy (`crates/postproc/src/tidy.rs`: stutter/bigram-repeat collapse, filler removal, sentence caps, terminal `.`/`?` heuristic — always on, works with LLM off) → snippet match (snippet SHORT-CIRCUITS the LLM).
6. **LLM cleanup** — `CleanupService::cleanup` via Ollama; guard can reject → falls back to raw. Skipped for terminals/identifiers. → `timing.llm_ms`
7. Focus re-check — if the focused app changed since key-down, injection is ABORTED (never paste into wrong window).
8. Injection — clipboard paste (`win_clipboard.rs`) or keystrokes for terminals (`win_keystroke.rs`). → `timing.inject_ms`
9. History insert (SQLite, `openflow-store`) + `on_timing` callback → Latency settings page.

## Latency triage — "it took N seconds" questions

Per-stage timings are RECORDED for every dictation: `DictationTiming {audio_ms, stt_ms, llm_ms, inject_ms, total_ms}`
→ visible in **Settings → Latency page** and in history rows (SQLite). **Ask the user for / look at these numbers first — they name the guilty stage without reading any code.**

| Symptom | Likely stage | Where to look |
|---|---|---|
| ALL decodes slow on a low-RAM machine (~20s finals) | Memory pressure: check free RAM + whether Ollama autostarted (`HKCU:...\Run` key). Whisper decode state is allocated ONCE per engine and reused (`whisper.rs`); if that regresses to per-call `create_state`, page-fault storms return | `crates/stt/src/whisper.rs`; Task Manager |
| Slow only on FIRST dictation after launch | Cold model pages (engines are warm-decoded at startup — `warm_up` in `lib.rs`/`whisper.rs`) or Ollama cold model load | shell init in `lib.rs`; `ollama.rs` (60s request timeout) |
| `llm_ms` dominates | Ollama slow/cold/wrong model | `crates/llm/src/ollama.rs`, `cleanup.rs`; `ollama-setup` skill |
| `stt_ms` dominates and grows with utterance length | StreamFinalizer not running (finalizer=None) or chunks not committing | `crates/core/src/finalizer.rs` (COMMIT_AFTER=5s, HOLDBACK=1.2s, CHUNK_THREADS=4, 400ms poll) |
| `stt_ms` high but flat | Model too big for CPU / thread config | `crates/stt/src/whisper.rs`; model choice in Settings → Models |
| Delay before recording even starts | hotkey hook or audio device init | `windows_hook.rs`, `crates/audio/src/engine.rs` |
| Whole SYSTEM sluggish while speaking (or thermal shutdown) | decode threads oversubscribing the CPU — check the "decode budget" startup log line matches the machine tier | `crates/stt/src/perf.rs` |
| Whole app frozen | orchestrator loop blocked (it is single-threaded) | `orchestrator.rs` — anything slow inside `finish_recording` blocks next dictation |
| Injection lands in wrong app / nothing appears | focus-abort triggered or injector failed | orchestrator.rs focus re-check; `crates/inject/src/win_*.rs` |
| Ollama down entirely | 3s connect timeout → falls back to raw text (still works, just no cleanup) | `ollama.rs:38` |

## Flow 2 — Voice commands ("make that shorter", assistant questions)

Same capture as Flow 1 through STT; then `try_run_command` (orchestrator.rs:496):
`match_command` (crates/llm/src/command.rs) matches trigger phrase → operand = current selection, else last injection → `run_command` via Ollama → output mode: **Replace/Insert** (inject over selection) or **Popover** (Assistant window, `on_assistant` → Assistant.tsx). Failure flashes "Command failed — is Ollama running?". A matched command NEVER falls through to dictation.

## Flow 3 — Live captions (partials)

`PartialWorker` (crates/core/src/partial.rs): own thread + own SMALL stt engine (**whisper-tiny-multi**, multilingual), re-decodes a sliding 5 s window every **350 ms**, capped at 3 threads. Receives the session language from the orchestrator; with auto-detect it pins the first detected language for the rest of the session (detection costs an extra pass per decode). Emits to HUD (Hud.tsx) only — nothing downstream consumes partials; final pass is authoritative.

## Flow 3b — Languages (Tamil/Telugu/auto-detect)

Config `language` ("auto" | "en" | "ta" | "te" | …) → `build_opts` →
`TranscribeOpts.language` (None = auto). `whisper.rs` passes "auto" to
whisper.cpp when None (an unset language silently decodes English — the old
bug) and reports the detected language in `Transcript.language`. Non-English
NEEDS a multilingual model (`whisper-*-multi` in registry.toml); the default
`whisper-base` is ggml-base.EN. Finalizer chunks pin the first detected
language like partials do. General.tsx shows a hint when a non-English
language is picked.

## Flow 4 — Streaming finalizer (why long dictations finish fast)

`StreamFinalizer` (crates/core/src/finalizer.rs): own thread SHARING the final engine (mutex, held per-decode). Poll every 400 ms; once ≥5 s undecoded audio pending, cut at quietest 80 ms window (never through a word, 1.2 s holdback), decode chunk with committed-text tail as prompt, 4 threads. On release `finish()` returns committed text + boundary; orchestrator decodes only the remainder.

## Flow 5 — Config changes (Settings UI → engine swap)

Settings page → `set_config` IPC (commands.rs) → save via `openflow-settings` (atomic write + migration) → `SessionEvent::ConfigChanged` → `Orchestrator::apply_config` (orchestrator.rs:175): mic device change rebuilds AudioEngine (only when Idle); STT model change rebuilds engine via `stt_factory` (downloads if missing, keeps old model on failure). Hotkey/LLM/other settings take effect from `self.config` next session.

## Flow 6 — Model download

Settings → Models page → `download_model` / `list_models` IPC → `openflow-models` registry (`registry.rs`) → checksummed, resumable download (`download.rs`) with progress events to UI. Ollama models are pulled via `openflow-models/src/ollama.rs` (see `ollama-setup` skill).

## Flow 7 — UI surfaces & events (shell, no logic)

- HUD (Hud.tsx): status Idle/Recording/Processing + live partials + mic level (`get_level`, 66 ms poll in lib.rs). Clickable Wispr-style pill: idle handle hover-expands to "Click or hold <chord> to start dictating" (chord from `get_config` + live `config` event); recording pill has ✕ (cancel) and red stop (finish-and-paste) buttons. The window stays WS_EX_NOACTIVATE (clicks never steal focus, so injection still targets the user's app); selective click-through = frontend reports clickable rects via `hud_zones` IPC, `start_hud_hit_testing` (lib.rs) polls the cursor at ~30 Hz and toggles `set_ignore_cursor_events`.
- Settings (Settings.tsx + pages/): all data via IPC commands in commands.rs — thin wrappers over crates.
- Assistant (Assistant.tsx): popover answers from Flow 2.
- Tray: status icon; opens Settings.

## Key defaults (change = update this file)

| Knob | Value | Where |
|---|---|---|
| Hotkey | RCtrl (Hold mode; Toggle + HandsFree available) | settings schema |
| Hands-free tick / silence hold / min peak | 200 ms / 2 s / 0.015 RMS | orchestrator.rs consts |
| Decode budget (partial interval/threads, chunk threads, tail threads, commit-after) | ADAPTIVE via `PerfProfile::get()` — Constrained (≤4 cores or ≤8 GB RAM): 600 ms / 2 / 3 / cores−2 / 3 s; Standard: 350 ms / 3 / 4 / cores−1 / 5 s | `crates/stt/src/perf.rs` (Meetily-style tiers; logged at startup as "decode budget") |
| GPU STT | opt-in `gpu-vulkan` cargo feature (whisper.cpp Vulkan; CI installs LunarG SDK on the runner and release builds ship with it; local dev builds are CPU unless the SDK is installed). Final engine GPU, partials engine ALWAYS CPU (`from_file_cpu`) so captions never contend. Startup log "final stt engine" shows the backend | `crates/stt/src/whisper.rs`, `.github/workflows/build-app.yml` |
| Tail decode language | pinned from first finalizer chunk (`Committed.language`) when config = auto, skipping whisper's per-decode detection pass; tail prompt = committed-text tail for continuity | orchestrator.rs `finish_recording` |
| Finalizer poll / commit threshold / holdback | 400 ms / 5 s / 1.2 s | finalizer.rs consts |
| Min utterance | 300 ms | orchestrator.rs:282 |
| Ollama connect / request timeout | 3 s / 60 s | ollama.rs |
| Trailing space after dictation | yes (not after identifiers) | orchestrator.rs:407 |
