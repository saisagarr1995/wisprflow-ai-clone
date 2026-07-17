# Session notes

Newest first. One entry per working session: what was done, challenges hit,
how they were solved, and what's next. Keep entries short — this file is
read at the start of future sessions.

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
