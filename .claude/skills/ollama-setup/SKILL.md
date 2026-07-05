---
name: ollama-setup
description: Ensure Ollama is installed, running, and has OpenFlow's models pulled so AI cleanup and voice commands work. Use when the Ollama status card is red/amber, when cleanup falls back to raw text, or when setting the app up on a new machine.
---

# Ollama setup for OpenFlow

OpenFlow's AI cleanup, formatting, and voice commands run through a **local
Ollama**. Dictation still works without it (raw transcript), but the polish
needs it. The app never fails if Ollama is down — it falls back to raw text.

## One-shot setup

Run `scripts/ensure_ollama.ps1`. It installs Ollama (winget) if missing, starts
the server, and pulls the model OpenFlow's config points at (default
`qwen3:4b`).

## Manual steps

1. Install: `winget install --id Ollama.Ollama` (or https://ollama.com).
2. Start the server: launch the Ollama app, or run
   `& "$env:LOCALAPPDATA\Programs\Ollama\ollama.exe" serve`.
3. Pull the model that matches `config.json` → `llm.cleanup_model`
   (default `qwen3:4b`): `ollama pull qwen3:4b`.
4. Verify: `curl http://localhost:11434/api/tags` returns 200 and lists the
   model. In the app, Settings → Models & AI → the status dot goes green.

## Notes

- Config lives at `%APPDATA%\openflow\config.json`. The `llm.cleanup_model` and
  `llm.rewrite_model` fields must be tags Ollama actually has (`ollama list`).
- Freshly-installed Ollama is often not on the current shell's PATH; call it by
  full path `$env:LOCALAPPDATA\Programs\Ollama\ollama.exe`.
- For higher-quality rewrites/commands, point `rewrite_model` at a larger model
  (e.g. `gpt-oss:20b` or `gemma3:12b`) and `ollama pull` it — it loads on demand.
