---
name: project-knowledge
description: Consult or maintain the project knowledge base — tree.md (auto-generated repo map) and flows.md (business flows + latency triage), both inside this skill folder. Use it to answer "where is X" and "why is Y slow/broken" WITHOUT globbing or reading pipeline code. Regenerate tree.md after structural changes; update flows.md after pipeline/behavior changes.
---

# Project knowledge base

Everything lives in this skill folder (`.claude/skills/project-knowledge/`):

| File | What it answers | Maintained |
|---|---|---|
| `tree.md` | **"Where is X"** — directory tree, every crate with purpose and modules, frontend files, Tauri IPC commands | auto-generated |
| `flows.md` | **"Why is Y slow / broken / behaving this way"** — every business flow step by step with `file:symbol` refs, latency-triage table (symptom → stage → file), timing knobs | by hand |
| `reference.md` | build-env quirks, architecture rules | by hand |
| `scripts/generate_tree.py` | regenerates `tree.md` | — |

## To answer a question (the token-saving path)

- Bug/latency/behavior report (e.g. "RCtrl takes 20 s") → read `flows.md`,
  use the triage table to pick the ONE file to open. Per-stage timings
  (`stt_ms`, `llm_ms`, …) are recorded for every dictation and shown on the
  Settings → Latency page — ask for those numbers before reading any code.
- "Where is X" → read `tree.md`.
- Do NOT glob/grep/read pipeline code for questions these files answer.

## To maintain

- `tree.md` (auto): `python .claude/skills/project-knowledge/scripts/generate_tree.py`
  from the repo root — run after adding/moving/renaming/deleting files,
  crates, or IPC commands.
- `flows.md` (manual): edit after changing a pipeline stage, event, timing
  knob, or adding a feature flow. Keep the same style: step lists with
  `file:symbol` refs, triage table rows, knob table.
- Do both at the END of any session that changed structure or behavior.
