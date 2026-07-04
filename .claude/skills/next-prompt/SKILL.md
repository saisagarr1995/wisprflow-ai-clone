---
name: next-prompt
description: Continue building OpenFlow by implementing the next prompt from the Fable 5 build plan (docs/FABLE5_PROMPT_PLAN.md), verifying it, committing, and updating the progress tracker. Use when the user says "continue the plan", "next prompt", or "keep building".
---

# Implement the next OpenFlow build prompt

1. **Locate position:** Read `.claude/PROGRESS.md` for the first prompt not marked ✅ done. Read that prompt's full text in `docs/FABLE5_PROMPT_PLAN.md`, plus any design-doc sections it references (`docs/ENGINEERING_DESIGN.md`).
2. **Implement** exactly the scoped prompt — goal, files, architecture, acceptance criteria, tests, refactoring notes. Respect the repo rules in `CLAUDE.md` (logic in `crates/*`, traits + fakes for hardware, single-threaded orchestrator).
3. **Verify:**
   - `cargo test --workspace` green
   - `cargo clippy --workspace --all-targets -- -D warnings` clean
   - `cargo fmt --all --check` clean
   - UI touched? `npm run build` in `apps/desktop/ui` passes.
   - Acceptance criteria that need hardware (mic, hotkey, injection): verify what's possible headlessly; list the remaining manual checks for the user explicitly in your summary.
4. **Record:** update `.claude/PROGRESS.md` (status + notes for any deviation), then commit with message `feat(prompt-NN): <short description>` and push if a remote is configured.
5. **Report:** summarize what was built, test results, and the manual verification steps the user should do.

Do exactly one prompt per invocation unless the user asks for more.
