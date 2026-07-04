# Fable 5 Development Plan — Incremental Build Prompts

Companion to [ENGINEERING_DESIGN.md](ENGINEERING_DESIGN.md). Feed these prompts to Fable 5 (Claude Code) **in order**; each builds on the previous state of the repo. Conventions for every prompt:

- Repo is a Cargo workspace per §13 of the design doc; Tauri 2 app in `apps/desktop`, logic in `crates/*`.
- Every prompt ends with: `cargo test --workspace` green, `cargo clippy -- -D warnings` clean, and (once UI exists) `npm test` green.
- Hardware-dependent code (audio, injection, hooks, UIA) must sit behind a trait with a fake implementation so tests run headless in CI.
- When a prompt says "per design doc §N", paste that section into the prompt context.

**Suggested session workflow per prompt:** plan mode first → implement → `/verify` the runtime behavior → commit.

---

## Phase 1 — Basic dictation (Prompts 1–8)

### Prompt 1 — Workspace scaffold
> **Goal:** Create the Cargo workspace + Tauri 2 skeleton per §13 of docs/ENGINEERING_DESIGN.md: crates `core`, `audio`, `stt`, `llm`, `postproc`, `context`, `inject`, `hotkey`, `settings`, `store`, `models` (empty lib.rs each with a doc comment stating its responsibility), and `apps/desktop` as a Tauri 2 app with React+TS+Vite+Tailwind UI showing a placeholder settings window and a system tray icon with Quit.
> **Files:** root `Cargo.toml` workspace, all crate manifests, `apps/desktop/*`, `.github/workflows/ci.yml` (build+test matrix win/mac/linux), `rustfmt.toml`, `.gitignore`.
> **Architecture:** apps/desktop contains no logic — only Tauri wiring.
> **Acceptance:** `cargo build --workspace` succeeds on Windows; `npm run tauri dev` shows tray + window; CI workflow compiles.
> **Tests:** one smoke test per crate (`it_links()`); CI runs them.
> **Refactor notes:** none — keep it minimal, no premature abstractions.

### Prompt 2 — Settings manager
> **Goal:** Implement `crates/settings`: strongly-typed `Config` (serde) with schema `version` field, hotkey string, mic device id, STT model id, language, LLM model names, injection strategy; load from `{config_dir}/openflow/config.json`, create with defaults if missing, atomic save (write-temp-rename), migration hook (`migrate(v_from, json) -> json`), and hot-reload via `notify` file watcher emitting `ConfigChanged` on a `tokio::sync::watch` channel.
> **Files:** `crates/settings/src/{lib,schema,io,watch,migrate}.rs`.
> **Acceptance:** corrupt/missing file → defaults + backup of corrupt file; edit on disk → watch fires within 1 s.
> **Tests:** round-trip serde; migration from a fake v0; atomic-save leaves no partial file on simulated failure; watcher integration test with tempdir.
> **Refactor notes:** all other crates receive `watch::Receiver<Config>` — no crate reads the file directly.

### Prompt 3 — Audio capture engine
> **Goal:** Implement `crates/audio`: `AudioEngine` using cpal — enumerate input devices, open selected device, capture → resample to 16 kHz mono f32 via rubato, push into a lock-free SPSC ring buffer (30 s). Continuous capture with `start_session()/end_session()` markers that snapshot the buffer including 250 ms pre-roll. Define `trait AudioSource` and add `FakeAudioSource` that plays WAV fixtures.
> **Files:** `crates/audio/src/{lib,engine,ring,resample,devices,fake}.rs`; `tests/fixtures/*.wav` (generate 3 short spoken-speech clips or sine sweeps).
> **Architecture:** capture callback does memcpy only — no allocation, no locks, no logging on the audio thread.
> **Acceptance:** on real hardware (manual), 5 s capture produces a 16 kHz mono buffer of correct length; device list shows names.
> **Tests:** resampler correctness (441→160 sample ratio on synthetic sine, check frequency); ring buffer wraparound; pre-roll length; all via FakeAudioSource.
> **Refactor notes:** expose `watch`-able RMS level for the future mic meter.

### Prompt 4 — VAD service
> **Goal:** Implement Silero VAD v5 in `crates/audio::vad` via `ort` (ONNX Runtime): `VadService::process(frame_32ms) -> SpeechProb`, plus an edge-trimmer (`trim_silence(buffer) -> Range`) and a hands-free end-pointer (`SilenceTimeout(700ms) -> UtteranceEnd` event). Model file loaded from `crates/models` registry path (Prompt 7 will download; for now check a local path and skip gracefully).
> **Files:** `crates/audio/src/vad.rs`; fixture with speech+silence.
> **Acceptance:** on the speech fixture, trimmed range excludes >90% of leading/trailing silence.
> **Tests:** synthetic silence → prob <0.1; fixture speech region → >0.5; end-pointer fires once after last speech frame + timeout.
> **Refactor notes:** keep VAD behind `trait Vad` — TEN-VAD may replace Silero later.

### Prompt 5 — STT engine trait + whisper.cpp backend
> **Goal:** Implement `crates/stt`: `trait SttEngine { fn transcribe(&self, audio: &[f32], opts: TranscribeOpts) -> Transcript }` where `TranscribeOpts` includes `language: Option<Lang>` and `initial_prompt: Option<String>`; `Transcript` has text + word timestamps + language. Backend 1: whisper.cpp via `whisper-rs`, model `large-v3-turbo-q5` (and `tiny` for tests), GPU auto-detect (CUDA/Vulkan/Metal) with CPU fallback, exposing which path is active. Add `FakeStt` returning canned transcripts.
> **Files:** `crates/stt/src/{lib,engine,whisper,fake}.rs`.
> **Acceptance:** manual: fixture WAV of read speech transcribes with ≥90% word accuracy on tiny model; log line states backend (CPU/CUDA/Vulkan).
> **Tests:** trait contract tests against FakeStt; whisper smoke test behind `#[ignore]` feature flag `models-installed` (CI skips, local runs).
> **Refactor notes:** `initial_prompt` is the dictionary-bias hook — thread it through now even though unused.

### Prompt 6 — Hotkey listener
> **Goal:** Implement `crates/hotkey`: low-level keyboard hook (Windows `WH_KEYBOARD_LL` via windows-rs; stub trait impls for mac/linux compiling to no-op) emitting `HotkeyEvent::{Pressed, Released}` for a configurable chord that supports modifier-only bindings (e.g. right-Ctrl alone) and both hold and toggle semantics. Parser for hotkey strings ("RCtrl", "Ctrl+Shift+Space"). Fake source for tests.
> **Files:** `crates/hotkey/src/{lib,parse,windows,fake}.rs`.
> **Architecture:** hook thread → mpsc → consumers; never do work on the hook callback (Windows kills slow hooks).
> **Acceptance:** manual: holding the configured key fires Pressed once, Released on key-up, even when another app has focus; no key-repeat spam.
> **Tests:** chord parser table tests; debounce/repeat suppression with fake event stream.
> **Refactor notes:** the same hook will later detect "foreground app changed during dictation" — keep the thread generic over event kinds.

### Prompt 7 — Model manager
> **Goal:** Implement `crates/models`: registry (static TOML) of STT/VAD models {id, url, sha256, size, license}; `ensure(model_id)` downloads with resume + progress callback + checksum verify into `{data_dir}/openflow/models/`; Ollama check: GET `/api/tags`, report missing LLM models, `pull(model)` proxying Ollama's pull with progress.
> **Files:** `crates/models/src/{lib,registry,download,ollama_check}.rs`; `registry.toml` (silero-vad-v5, whisper-tiny, whisper-large-v3-turbo-q5, parakeet-tdt-0.6b-v3-onnx).
> **Acceptance:** fresh machine → `ensure()` downloads and verifies; corrupted file re-downloads; Ollama down → clear error, not a panic.
> **Tests:** checksum failure path with local HTTP test server (axum); resume from partial file; registry parse.
> **Refactor notes:** progress events go on the core event bus for the future UI download manager.

### Prompt 8 — Orchestrator v1 + clipboard injection = first dictation
> **Goal:** Wire it together. `crates/core`: `SessionOrchestrator` — single-threaded event loop over one mpsc receiving {Hotkey, AudioReady, TranscriptReady, ConfigChanged}; state machine Idle→Recording→Transcribing→Injecting→Idle with explicit `enum State` and illegal-transition logging. `crates/inject`: `trait Injector`; `ClipboardPasteInjector` for Windows (arboard save/restore incl. delay from config, SendInput Ctrl-V via windows-rs) + `FakeInjector` recording calls. Tray: icon state (idle/recording). Hold hotkey → speak → release → transcript pasted into focused app.
> **Files:** `crates/core/src/{lib,orchestrator,state,events}.rs`, `crates/inject/src/{lib,clipboard_win,fake}.rs`, wiring in `apps/desktop/src-tauri`.
> **Architecture per design doc §5:** orchestrator is the only owner of session state; services are trait objects injected at construction.
> **Acceptance (the phase-1 demo):** dictate a sentence into Notepad, Chrome address bar, and Slack; text appears within 2 s of release; clipboard contents restored.
> **Tests:** full pipeline test: FakeAudio(fixture) + FakeStt + FakeInjector → orchestrator emits exactly one inject with expected text; state machine property test (random event sequences never panic, always return to Idle).
> **Refactor notes:** this is the load-bearing loop — add tracing spans per stage NOW (they feed the latency monitor later).

---

## Phase 2 — Streaming (Prompts 9–12)

### Prompt 9 — Settings UI v1
> **Goal:** Build the real settings window: left-nav (General, Microphone, Models); hotkey capture widget (press keys to record chord, hold/toggle radio); mic picker with live level meter (RMS from audio crate via Tauri events); model download manager UI with progress bars; Ollama status card. Tauri commands + events bridging to `settings`/`models` crates.
> **Files:** `apps/desktop/ui/src/*` (React), `apps/desktop/src-tauri/src/commands.rs`.
> **Acceptance:** change mic → next dictation uses it without restart; download a model from UI; invalid hotkey chord rejected inline.
> **Tests:** vitest component tests for hotkey widget state machine; Rust command-layer tests with mocked services.
> **Refactor notes:** define a single typed IPC event enum shared via specta/ts-rs codegen — no stringly-typed events.

### Prompt 10 — Parakeet ONNX engine
> **Goal:** Add `ParakeetOnnx` backend to `crates/stt` per design doc §4: ONNX Runtime session for parakeet-tdt-0.6b-v3, greedy TDT decode, int8 where available, CUDA/CPU providers, `initial_prompt` mapped to hotword bias list (post-hoc fuzzy boost if native biasing unavailable). Engine selection in settings (auto: EN→Parakeet, else Whisper).
> **Files:** `crates/stt/src/parakeet/{mod,decode,features}.rs`.
> **Acceptance:** fixture transcription accuracy ≥ whisper-turbo on English fixtures; RTF < 0.05 on CPU (measure and print).
> **Tests:** decoder unit tests on saved logit fixtures (commit small .npy fixtures); language-routing table test.
> **Refactor notes:** mel/feature extraction shared with whisper backend → extract `stt::features` module.

### Prompt 11 — Streaming partials + two-pass finalization
> **Goal:** Add `trait StreamingStt { fn feed(&mut self, frames); fn partial(&self) -> String; fn finalize(self) -> Transcript }`. Parakeet: frame-synchronous incremental decode. Whisper: 500 ms re-decode of a sliding window with LocalAgreement-2 confirmed-prefix (port the policy from ufal/whisper_streaming, cite it in comments). Orchestrator: during Recording, partials stream as events; on release, run **final pass** on full utterance with `initial_prompt`, replacing partial text.
> **Files:** `crates/stt/src/streaming/{mod,local_agreement,parakeet_stream,whisper_stream}.rs`; orchestrator changes.
> **Acceptance:** manual: partials visibly track speech <500 ms behind; final text quality unchanged vs Prompt 8.
> **Tests:** LocalAgreement unit tests (agreeing/disagreeing decode sequences → correct committed prefix); streaming-vs-batch equivalence on fixtures (final text identical).
> **Refactor notes:** partials are UI-only — nothing downstream may consume unconfirmed text; enforce by type (`PartialText` newtype ≠ `FinalTranscript`).

### Prompt 12 — HUD overlay + latency spans
> **Goal:** Always-on-top transparent Tauri window (click-through, bottom-center, multi-monitor DPI aware): waveform from RMS events, live partial text (last 8 words), state pill (recording/transcribing/formatting/done), auto-hide. Latency: tracing layer that captures per-stage spans into a ring of `DictationTiming` records stored in `store` (add SQLite via rusqlite with migrations: `timings` table). Dev-mode HUD shows stage breakdown.
> **Files:** `apps/desktop/ui/src/hud/*`, new `crates/store` with `migrations/`, tracing layer in `core`.
> **Acceptance:** HUD appears on key-down over any app incl. fullscreen browser; never steals focus (critical — injection depends on focus staying put); timings recorded per dictation.
> **Tests:** store migration + insert/query tests; span-capture layer unit test; HUD component tests.
> **Refactor notes:** "never steals focus" needs `WS_EX_NOACTIVATE` on Windows — document in code why.

---

## Phase 3 — LLM cleanup (Prompts 13–17)

### Prompt 13 — Ollama client + model router
> **Goal:** Implement `crates/llm`: `trait LlmBackend { async fn chat(&self, req) -> TokenStream }`; `OllamaBackend` (reqwest, `/api/chat`, streaming NDJSON, `keep_alive:-1`, `format: json_schema` support, timeout+retry-once, health check); `ModelRouter` mapping `TaskKind::{Cleanup, Command, Rewrite, Coding, Assistant}` → model name from config with VRAM-tier defaults per design doc §3. Pre-warm on startup (1-token generate).
> **Files:** `crates/llm/src/{lib,backend,ollama,router,types}.rs`.
> **Acceptance:** with Ollama running qwen3:4b-instruct, a cleanup request streams tokens; Ollama down → typed `LlmUnavailable` error.
> **Tests:** NDJSON stream parser on recorded fixtures (incl. mid-token chunk splits); router table; mock-server integration (wiremock).
> **Refactor notes:** everything above this speaks `LlmBackend` — llama.cpp-server backend should be addable in one file later.

### Prompt 14 — Prompt pipeline + cleanup prompt + output guard
> **Goal:** `crates/llm::prompts`: template engine (minijinja) with variables `{app_profile, dictionary, preceding_text, examples, transcript}`; the hot-path **cleanup prompt** per design doc §3 (fillers, false starts, self-corrections "actually/no wait", punctuation, paragraphs, list detection, never answer questions, no preamble, preserve dictionary terms); **output guard**: strip preambles/fences, reject on length ratio >2.5×/<0.3×, meta-text patterns, or mangled dictionary terms → fall back to deterministic transcript with a `GuardTripped` event. Skip-LLM fast path (<4 words, no fillers regex).
> **Files:** `crates/llm/src/{prompts/mod.rs, prompts/cleanup.j2, guard.rs}`; orchestrator gains Formatting state between Transcribing and Injecting.
> **Acceptance:** dictating "um so basically we should uh ship it on friday no wait monday" injects "We should ship it on Monday."; dictating "what time is the meeting" injects the question verbatim (not an answer).
> **Tests:** a **golden-file suite**: `tests/fixtures/cleanup/*.txt` pairs (raw → expected) run against the guard + prompt-render (LLM call mocked with recorded responses); guard rejection cases; fast-path routing.
> **Refactor notes:** golden suite becomes the regression net for all future prompt edits — make adding a case one file.

### Prompt 15 — Personal dictionary
> **Goal:** `crates/store` tables `dictionary(word, hint, created)`; `crates/postproc`: fuzzy matcher (normalized edit distance + metaphone phonetic key) replacing ASR near-misses pre-LLM; wire into `initial_prompt` (Whisper) / hotword list (Parakeet) and cleanup-prompt `{dictionary}`. Settings UI: Dictionary page (CRUD, import/export CSV).
> **Files:** `crates/postproc/src/{lib,dictionary,phonetic}.rs`, store migration, UI page.
> **Acceptance:** add "kubectl" → dictate "cube control apply" → `kubectl apply` injected.
> **Tests:** matcher precision table ("cube control"→kubectl yes; "cube"→no); phonetic key cases; round-trip import/export.
> **Refactor notes:** matcher must be O(dict × words) with early-exit — benchmark test at 500 dictionary entries <2 ms.

### Prompt 16 — Snippets
> **Goal:** `snippets(trigger, expansion, profile_scope)` table; postproc detects trigger phrases in the final transcript (normalized match, must be whole utterance or "insert <trigger>") → expansion short-circuits LLM; expansion supports `{cursor}` placeholder (inject text, then send Left-arrow keys to position caret). Settings UI: Snippets page.
> **Files:** `crates/postproc/src/snippets.rs`, inject caret-positioning support, UI page.
> **Acceptance:** trigger "my address" inserts the saved block, caret lands at `{cursor}`.
> **Tests:** trigger matching (exact, case, partial-utterance negative); cursor-offset key-count math incl. multi-line.
> **Refactor notes:** snippet detection order is before dictionary replace — document the postproc pipeline order in one place (`postproc::PIPELINE` doc).

### Prompt 17 — History + logs UI
> **Goal:** `history(raw, formatted, app, profile, timings, created)` table (content only — never screen context); History page with search, raw↔formatted diff view, re-copy button, delete/clear-all; Logs page tailing the tracing file with level filter; "copy debug bundle" (logs + config with secrets redacted — there are none, but keep the redact hook).
> **Files:** store migration, UI pages, tracing file appender config.
> **Acceptance:** every dictation appears in history; diff view highlights LLM edits; clear-all wipes table.
> **Tests:** store queries + FTS search; retention cap (configurable max rows) pruning.
> **Refactor notes:** history is the future style-learning corpus — keep `formatted_final` vs `user_corrected` columns ready (nullable) now.

---

## Phase 4 — Context awareness (Prompts 18–20)

### Prompt 18 — Focus & app identity
> **Goal:** `crates/context`: `trait ContextProvider`; Windows impl: foreground process name + window title (Win32), captured async at key-down with 150 ms timeout; `AppProfileMatcher`: rules table (process/title regex → profile enum {Chat, Email, Coding, Document, Terminal, Generic}) with user-extensible rules in config; profile switches the cleanup prompt's tone block. Foreground-change-during-dictation detection → abort injection with HUD warning.
> **Files:** `crates/context/src/{lib,provider,windows_focus,profiles}.rs`.
> **Acceptance:** dictation into Slack yields casual output, into Gmail (Chrome tab, title-matched) yields formal; switching windows mid-dictation does not paste into the wrong app.
> **Tests:** matcher rules table (20 cases: slack.exe, "- Gmail - Chrome", Cursor.exe, wt.exe…); timeout path returns Generic without blocking.
> **Refactor notes:** profile → prompt selection lives in `PromptPipeline`, not scattered ifs.

### Prompt 19 — Nearby text via UI Automation
> **Goal:** Windows UIA in `crates/context`: focused element → `TextPattern` (text around caret, current selection) or `ValuePattern`, ~1000 chars, async with timeout, graceful None for terminals/games; entity extraction (capitalized-sequence + camelCase regex heuristics) → bias list into STT `initial_prompt` + "preserve exactly" prompt block; `preceding_text` (200 chars before caret) into the cleanup prompt for tense/continuation.
> **Files:** `crates/context/src/{win_uia,entities}.rs`.
> **Acceptance:** with "Dr. Nayarti's follow-up" visible in the compose box, dictating the name transcribes it correctly; selection capture returns the selected string.
> **Tests:** entity extractor table tests; UIA behind trait — orchestrator tests use `FakeContext`; manual checklist doc for Word/Chrome/VS Code/terminal.
> **Refactor notes:** context text must never reach `store` — add a test asserting history rows contain no context fields.

### Prompt 20 — Profiles system + auto-switch
> **Goal:** Full profiles per design doc §5: named bundles {hotkey?, stt model, llm routing, prompt template, injection strategy, language} in config; auto-activation rules (app-profile → user profile); Profiles UI page (CRUD, duplicate, set-default) + tray quick-switcher; per-profile prompt editor with template variables and a "test with sample transcript" button (runs real LLM, shows before/after + token count + ms).
> **Files:** `crates/settings` profile schema + resolution, UI pages, tray menu.
> **Acceptance:** create "Email" profile with custom prompt → focusing Gmail auto-activates it (HUD shows profile name); prompt tester round-trips.
> **Tests:** profile resolution precedence (explicit > app-rule > default); config migration adding profiles.
> **Refactor notes:** resolved `EffectiveConfig` computed in one function, passed immutably into each session — no mid-session config reads.

---

## Phase 5 — Developer mode (Prompts 21–22)

### Prompt 21 — Casing engine + code grammars
> **Goal:** `crates/postproc::casing` per design doc §9: spoken casing commands (camel/pascal/snake/kebab/screaming/dot) applied to following phrase; path grammar ("src slash components slash button dot tsx" → `src/components/Button.tsx`); symbol vocabulary (arrow, pipe, backtick, brackets…) with per-language table; terminal grammar (dash-M → `-m`, "pipe" → `|`); number/ordinal handling. Pure functions, exhaustive table tests.
> **Files:** `crates/postproc/src/casing/{mod,commands,paths,symbols,terminal}.rs`.
> **Acceptance:** the 30-case table in the module doc all pass, e.g. "get user by ID camel case" → `getUserById`.
> **Tests:** the 30-case table + property test (casing round-trips words losslessly modulo separators).
> **Refactor notes:** runs only when Coding profile active; ordering: casing after dictionary, before LLM.

### Prompt 22 — Coding profile + IDE awareness
> **Goal:** Coding cleanup prompt variant (no prose prettifying, preserve symbols/spacing, never smart-quote — add golden cases); language detection from window title extension; per-language casing defaults; route Coding rewrites to `qwen3-coder` when configured; terminal-profile injection forces SendInput-Unicode (never paste into terminals by default — design doc §8); optional: docs for a future VS Code companion extension (socket protocol spec only, no implementation).
> **Files:** prompt template, `context` language detect, router + injection strategy table entries, golden cases, `docs/adr/ide-extension-protocol.md`.
> **Acceptance:** in VS Code, dictating a comment keeps code words intact; in Windows Terminal, injection types (not pastes) and never auto-executes (strip trailing newline!).
> **Tests:** golden coding cases; injection-strategy selection table; trailing-newline strip test.
> **Refactor notes:** "never send Enter to a terminal" is a safety invariant — encode as a unit-tested guard in `inject`, not prompt-level hope.

---

## Phase 6 — Voice commands (Prompts 23–25)

### Prompt 23 — Intent gate + structured command parsing
> **Goal:** `crates/postproc::intent` per design doc §10: Tier-0 regex gate (command verbs + selection/last-injection present); Tier-1 LLM classification with Ollama structured outputs (JSON schema `{intent, action, args}`), only for ambiguous heads; plumb `Selection` and `LastInjection` through orchestrator state.
> **Files:** `crates/postproc/src/intent.rs`, `crates/llm/src/schemas.rs`, orchestrator.
> **Acceptance:** "rewrite this professionally" with selection → classified transform in <10 ms (Tier-0); plain dictation never hits Tier-1 (assert via metrics counter).
> **Tests:** Tier-0 table (25 utterances → intent); schema deserialization of malformed LLM JSON falls back to dictation (never lose the user's words).
> **Refactor notes:** fail-open to dictation — a mis-fired command that injects the transcript beats one that eats it.

### Prompt 24 — Command registry + built-in transforms
> **Goal:** Data-driven `CommandRegistry` {name, patterns, prompt template, task-kind (→model), output mode: ReplaceSelection|InsertAtCaret|Popover}; built-ins: rewrite-professionally, summarize, shorten, expand, translate-to-X, to-email, fix-grammar, to-bullets, to-markdown, explain(popover); execution path: selection → LLM → injection replace (select-all not needed — selection is replaced by paste); undo tracking (last injection length + strategy) with "undo dictation" hotkey/command.
> **Files:** `crates/core/src/commands/{registry,exec}.rs`, prompt templates per command, HUD flash messages.
> **Acceptance:** select a paragraph, hold hotkey, say "make it shorter" → paragraph replaced with a shorter version; "undo dictation" restores.
> **Tests:** registry resolution incl. translate-language arg extraction; exec pipeline with fake LLM/injector; undo byte-length math for multi-byte text.
> **Refactor notes:** commands and cleanup share the output guard — generalize guard configs per task kind.

### Prompt 25 — Custom commands UI + assistant popover
> **Goal:** Settings page to CRUD custom commands (trigger phrases, prompt template with `{selection}`, model choice, output mode) persisted in store; assistant mode: "ask local AI …" / "explain this" streams the answer into a dismissable always-on-top popover (markdown-rendered, copy button) — never injected.
> **Files:** UI pages, popover window, store migration `commands` table.
> **Acceptance:** user-created command "make it pirate" works end-to-end; "ask local AI what is a monad" streams into popover.
> **Tests:** command CRUD + validation (duplicate trigger rejected); popover component stream rendering.
> **Refactor notes:** registry merges built-ins + user commands with user precedence — one merge function, tested.

---

## Phase 7 — Performance (Prompts 26–27)

### Prompt 26 — Latency hardening
> **Goal:** Implement design doc §11: pre-warm STT+LLM at startup and after config change; stable prompt-prefix ordering for Ollama prompt-cache hits (system+context+dictionary before transcript; verify with `prompt_eval_count` deltas); context snapshot moved fully parallel to recording; skip-path counters; add `openflow bench` CLI subcommand: runs N fixture dictations through the real pipeline (fake audio/injector, real STT+LLM) and prints per-stage p50/p95 table.
> **Files:** warmup module, prompt ordering refactor, `apps/desktop/src-tauri/src/bench.rs` (or `crates/core/src/bench.rs`).
> **Acceptance:** on dev machine (document specs in README): p50 release→inject ≤500 ms, p95 ≤800 ms for 10 s utterances with Parakeet + qwen3:4b; bench command prints the table.
> **Tests:** prompt-prefix stability test (same session → byte-identical prefix); bench harness smoke on fixtures with fakes.
> **Refactor notes:** store bench results as JSON artifacts so regressions are diffable in CI (manual trigger job).

### Prompt 27 — Latency monitor UI + sentence-streaming injection (opt-in)
> **Goal:** Latency page: waterfall per stage for last 50 dictations, p50/p95 trend chart, model tok/s, active compute path (CUDA/Vulkan/CPU) badges; opt-in "progressive injection": inject LLM output sentence-by-sentence as tokens stream (guard: only for Generic/Document profiles, never Coding/Terminal; abort→backspace on guard trip is NOT attempted — document why: leave partial text + HUD warning instead).
> **Files:** UI page (use `dataviz` conventions), `inject` progressive mode, config flag.
> **Acceptance:** monitor reflects real dictations live; progressive mode visibly reduces perceived latency in Notepad; guard trip leaves a HUD warning.
> **Tests:** sentence-boundary chunker (abbreviations, decimals, ellipses); progressive-mode profile gating table.
> **Refactor notes:** chunker shared with future TTS — put in `postproc::sentences`.

---

## Phase 8 — Polish (Prompts 28–30)

### Prompt 28 — macOS parity
> **Goal:** Implement mac backends behind the existing traits: hotkey (CGEventTap incl. Fn/Globe), injection (NSPasteboard swap + Cmd-V CGEvent, CGEvent-Unicode fallback, AX insertion opportunistic), context (AXUIElement focus/value/selection, NSWorkspace app identity), permissions onboarding (Microphone + Accessibility prompts with re-check screen), whisper.cpp Metal + Parakeet CoreML/CPU.
> **Files:** `*/src/macos_*.rs` across hotkey/inject/context, onboarding UI step.
> **Acceptance:** full phase-1-through-6 manual checklist passes on an M-series Mac; permissions flow recovers from "denied then granted".
> **Tests:** existing trait contract tests run against mac fakes; CI adds macos target build + unit tests.
> **Refactor notes:** any `#[cfg(windows)]` leakage outside platform modules found during this port gets moved behind traits — budget for it.

### Prompt 29 — Linux parity (X11 + best-effort Wayland)
> **Goal:** X11: XTEST injection, XCB clipboard, active-window context, evdev/X11 hotkeys. Wayland: wtype (virtual-keyboard) with ydotool fallback, degrade context to Layer 1/2, document compositor matrix (GNOME/KDE/wlroots) in docs; PipeWire capture verified via cpal; AppImage + .deb packaging.
> **Files:** platform modules, `docs/linux-support.md`, bundler config.
> **Acceptance:** X11 full checklist passes; Wayland/wlroots basic dictation works; unsupported combos show actionable errors, not silence.
> **Tests:** trait contract tests; CI linux build + unit tests.
> **Refactor notes:** keep the Wayland matrix honest in docs — this is where users file 80% of Linux issues.

### Prompt 30 — Release engineering + onboarding
> **Goal:** Ship v1.0: NSIS installer + code-signing hooks, .dmg + notarization docs, Tauri signed auto-updater (GitHub Releases, opt-in); first-run wizard (mic test → hotkey choice → STT model download → Ollama detect/install-link + `ollama pull` of default models → test dictation into the wizard's own textbox); README, user docs, CONTRIBUTING, license (GPL-3.0 or MIT — decide and record ADR); **no-telemetry CI test**: integration run asserting zero non-localhost connections (mock DNS/socket audit); `--headless` CLI mode.
> **Files:** installer configs, `apps/desktop/ui/src/onboarding/*`, docs/, `.github/workflows/release.yml`, network-audit test.
> **Acceptance:** clean Windows VM: installer → wizard → first successful dictation in under 3 minutes with no manual config; updater detects a staged release.
> **Tests:** wizard state machine; updater manifest verification; the network-audit test in CI.
> **Refactor notes:** freeze the config schema at v1 and require migrations from here on.

---

## Backlog (post-v1 prompts, sketched)
- **P31 Style learning:** capture user edits to injected text (clipboard/UIA diff heuristic) → per-profile few-shot store → cleanup prompt; the design-doc §1.2 "correction learning" loop, fully local.
- **P32 Plugin system:** WASM (extism) or JSON-RPC subprocess plugins: custom context providers, postprocessors, commands; manifest + permission model.
- **P33 VS Code/Cursor companion extension** implementing the P22 socket protocol (symbols in scope, file, language).
- **P34 Multi-language routing polish:** per-utterance LID, per-language dictionaries, translate-while-dictating mode.
- **P35 Hands-free mode:** VAD end-pointing dictation without hotkey, wake-word (openWakeWord) optional.
