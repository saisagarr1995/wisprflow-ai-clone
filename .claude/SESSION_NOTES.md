# Session notes

Newest first. One entry per working session: what was done, challenges hit,
how they were solved, and what's next. Keep entries short — this file is
read at the start of future sessions.

## 2026-07-17 (later-2) — "instant" push: streaming + language pinning + Vulkan via CI (same branch)

**Status: code DONE + validated locally on CPU; GPU installer building in CI
(run 29589398160).** User: "main is instant, this branch takes 30s; want
real-time like Wispr". Measured reality: their real dictation showed
stt 4.76 s for ~6 s speech — CPU-bound base-model decode (RTF ≈ 0.8 on the
4700U), NOT a branch regression (debug builds still get optimized C++ —
whisper-rs-sys forces cmake Release always). Changes: (1) Constrained tier
`commit_after_ms` 3 s (Standard 5 s) → tail at release stays short even for
mid-length utterances; (2) `Committed.language` pins the chunk-detected
language into the tail decode (skips auto-detect pass) + tail prompt =
committed-text tail; (3) Latency.tsx seeds from `history_list` on mount
(was empty until first dictation) then live-appends `timing` events;
(4) `gpu-vulkan` cargo feature → whisper.cpp Vulkan (final engine GPU,
partials `from_file_cpu` ALWAYS CPU); build-app.yml installs pinned LunarG
SDK 1.4.313.2 on the RUNNER and builds `--features gpu-vulkan`.
**User boundaries stated this session: no local software installs (they
cancelled the Vulkan SDK UAC prompt — do NOT retry locally), security
first, always present a plan first.** E2E validated via SAPI TTS through
speakers + synthetic RCtrl hold (volume-up keys first — initial run heard
nothing): "dictation injected stt_ms=4835 total_ms=5001 chars=40" into
Notepad. Next: when CI run finishes, download installer, silent-install
(/S, per-user), verify startup log says "whisper.cpp ... (Vulkan GPU)" and
re-measure stt_ms; expect ~3-5x faster finals on the Vega 7 iGPU.

## 2026-07-17 (later) — adaptive decode budget (same branch `feat/handsfree-pill-multilingual`)

**Status: DONE, verified live.** User reported the app slow again + a laptop
shutdown/restart, and asked to mine `meetily-main.zip`. Zip's one useful idea
= hardware-tier adaptive whisper config (its `HardwareProfile` caps threads
per tier). Root cause here: fixed thread counts tuned for a free machine —
partials (3) + finalizer chunks (4) overlap while speaking = 7/8 cores busy,
tail decode took 7; with ~0.8 GB free RAM that saturates the laptop
(plausible thermal shutdown). Fix: new `crates/stt/src/perf.rs`
`PerfProfile::get()` (OnceLock; cores via available_parallelism, RAM via
minimal GlobalMemoryStatusEx FFI — no sysinfo dep). Constrained tier
(≤4 cores or ≤8 GB RAM → THIS laptop): partials 2 thr @ 600 ms, chunks 3,
tail cores−2; Standard keeps old values. Wired in whisper.rs default,
partial.rs, finalizer.rs, lib.rs cadence. 5 unit tests; clippy clean.
Verified: startup log prints `decode budget ... tier: Constrained`, full
synthetic toggle-mode session cycled recording→processing→idle cleanly.
Environment note: machine had 0.8 GB free RAM (Chrome + Antigravity IDE +
Edge + Notion + Perplexity + Teams — user-side; Ollama autostart is GONE).
Wispr pill screenshots the user re-shared are ALREADY implemented (Flow 7).

## 2026-07-17 — hands-free + clickable pill + Tamil/Telugu (branch `feat/handsfree-pill-multilingual`)

**Status: IMPLEMENTED — awaiting user's on-device test + PR merge.**
(Branch is off `perf/resident-state-rule-tidy`; PR #11 still unmerged at
plan time, so this PR's diff shows both branches until #11 merges.)

User asked for (with Wispr Flow screenshots as reference):
hands-free mode; Tamil + Telugu with auto-detect; clickable HUD pill
(idle: "Click or hold Ctrl + Win to start dictating" → click starts; while
recording: X on the left cancels, red stop on the right finishes-and-pastes);
"Mode" in Settings → General reportedly not working; README polish; no
laptop hangs; full validation.

Root causes found up front:
- Tamil/Telugu/auto-detect **cannot work today**: registry only has
  English-only `ggml-*.en.bin` models, and `whisper.rs` never passes
  `language="auto"` (None → whisper defaults to en). Partials engine is
  tiny.en, hard-coded.
- HUD window is permanently click-through (`set_ignore_cursor_events(true)`);
  orchestrator has no cancel event and no way to start/stop from the UI.
- No hands-free mode; only Hold/Toggle.
- Mode plumbing (UI → set_config → watch channel → ConfigChanged →
  apply_config → on_hotkey_pressed) reads correct; needs runtime repro —
  user may be running the OLD installed build (PR #11 unmerged).

Plan / checklist:
- [x] Multilingual STT: `whisper-tiny-multi`/`-base-multi`/`-small-multi` in
      registry.toml (sha256 from HF LFS pointers); `whisper.rs` passes
      "auto" to whisper.cpp when language is None (unset = silent English —
      that was the bug) and reports the detected language; partials use
      multilingual tiny + inherit session language; partials/finalizer pin
      the first auto-detected language per session (detection costs an
      extra pass per decode); General.tsx adds ta/te + multilingual hint.
- [x] Core: `SessionEvent::{HudStart,HudFinish,HudCancel}`, `cancel_recording`,
      `HotkeyMode::HandsFree` (200 ms recv_timeout tick, level-based
      trailing-silence endpointing: 2 s hold, peak-relative threshold,
      0.015 min RMS peak). Tests: runtime mode-switch regression,
      hands-free auto-stop (realtime-paced fake source — a CPU-speed flood
      exhausts the noise before the session starts), hud start/finish/cancel,
      event-storm test extended to the new events. 18 core tests green.
- [x] Shell/HUD: orchestrator handle in AppState; `hud_start/finish/cancel` +
      `hud_zones` IPC; `start_hud_hit_testing` cursor poll (~30 Hz) toggles
      ignore_cursor_events so only reported rects are clickable (WebView2
      captures input even over transparent pixels — per-pixel alpha
      hit-testing does NOT work); window stays WS_EX_NOACTIVATE so pill
      clicks never steal focus from the injection target; `config` event
      emitted on hot-reload for the pill's chord text; Ctrl+Win preset.
- [x] Validate: cargo test --workspace green, clippy -D warnings clean,
      fmt clean, tsc + vite build clean, debug exe built, smoke-launched:
      "pipeline ready", synthetic RCtrl dictation end-to-end OK.
- [x] Mode bug: full chain re-verified + regression test; could not repro in
      current code — likely the user was on the OLD installed build
      (%LOCALAPPDATA%\OpenFlow, pre-PR-11). Live toggle-mode test via
      config.json hot-edit + synthetic keys done this session.
- [x] Docs: README (pill, hands-free, Tamil/Telugu step-by-step, config
      table), flows.md (Flow 1 modes/HUD events, Flow 3b languages, HUD
      zones), this entry.

## 2026-07-17 — 20s latency (round 2) + rule-based tidy (branch `perf/resident-state-rule-tidy`)

**Problem:** User again reported >20 s from RCtrl release to text, despite the
streaming finalizer (PR #8) being installed and previously verified at ~1-2 s.

**Diagnosis (do this order again next time):**
1. App wasn't running when checked (no autostart — known pitfall).
2. Ollama server+tray had autostarted again (Run key is back / never removed)
   → free RAM was **817 MB** of 7.4 GB.
3. Real code defect found: `WhisperEngine::transcribe` called
   `context.create_state()` **per decode** — ~100 MB alloc/free for every
   partial (every 350 ms), chunk finalize, and tail decode. Under RAM
   pressure this evicts the model weights between decodes → hard page-fault
   storm → 20 s decodes. This is why the machine "hangs".
4. Checked user's meetily-main.zip: their whisper server is stock whisper.cpp
   (greedy, base.en, min(4,cores) threads) — no magic; their useful idea is
   resource-adaptive config + resident engines, which we adopted the
   resident-state part of.

**Fix (this branch):**
- `crates/stt/whisper.rs`: create `WhisperState` once in `from_file`, reuse
  across all decodes (whisper-rs 0.16 state is owned + Send; it keeps the
  ctx alive via internal Arc, so the wrapper is dropped).
- `SttEngine::warm_up` (default no-op): 0.5 s silence decode; desktop shell
  warms the shared final engine on a background thread and the partial
  engine inline at bootstrap → first dictation no longer cold.
- `crates/postproc/tidy.rs` (**new**): deterministic Wispr-style cleanup with
  LLM off — collapses immediate word/bigram/trigram repeats (keeps later
  copy, never across sentence ends), drops fillers (um/uh/erm...), caps
  sentence starts + I/I'm/I'll, adds terminal `.` or `?` (question-opener
  heuristic). Wired in orchestrator prose path only (coding/terminal stay
  literal); runs before snippet match (safe — snippet matching is
  punctuation/case-insensitive) and before LLM cleanup when enabled.
- Updated 4 orchestrator test assertions ("hello world " → "Hello world. ").
- Updated `.claude/skills/project-knowledge/flows.md` (new tidy stage + new
  latency-triage row for the RAM-pressure symptom).

**Challenges / gotchas hit:**
- `git checkout main` blocked by untracked `image.png` (main still tracked
  it) — moved to scratchpad.
- Everything read pre-branch was on merged PR #8's branch; PRs #9/#10 landed
  meanwhile (screenshots removal + project-knowledge skill) — no code deltas,
  but always re-diff after branching.
- Classifier still blocks removing the Ollama Run key; killed the processes
  and told the user the `Remove-ItemProperty` command again.

**Outcome:** User tested the debug build on-device 2026-07-17 and confirmed
dictation is fast and working. PR #11 open (tests green, clippy clean) —
user to merge; installed app under %LOCALAPPDATA%\OpenFlow still runs the
OLD build until a CI installer from merged main is installed.

**Next / open:**
- User must remove Ollama autostart themselves (see command in PR/summary).
- If captions still feel slow on this box: consider sharing one whisper
  context between partial+final engines (saves tiny model's ~125 MB), or
  meetily-style adaptive degrade (skip partials when free RAM < ~700 MB).
- Parakeet engine (P10) remains the real sub-500 ms path.
