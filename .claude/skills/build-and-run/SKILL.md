---
name: build-and-run
description: Build, test, and launch OpenFlow on Windows with the correct toolchain environment (MSVC, CMake, LLVM/libclang). Use whenever asked to build, test, run, or relaunch the app, or when a build fails with a missing linker / cmake / libclang error.
---

# Build & run OpenFlow

The whisper.cpp dependency needs **CMake** and **LLVM/libclang** on PATH, and the
MSVC linker. `.cargo/config.toml` (gitignored) already sets `LIBCLANG_PATH` and
`CMAKE`, so plain `cargo` works. If a build ever fails with "program not found"
(cmake) or "libclang" or "link.exe not found", run `scripts/build.ps1` which
sets everything explicitly.

## Commands (from repo root)

- Test everything: `scripts/build.ps1 test`
- Lint: `scripts/build.ps1 clippy`
- Run the app (dev): `scripts/build.ps1 run`
- Production installer: `scripts/build.ps1 bundle`

Or directly, once the env is set:
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all`
- UI: `npm run build` (and `npm install` once) in `apps/desktop`

## Rules

- Build from **PowerShell**, not Git Bash (Git Bash's coreutils `link` shadows
  the MSVC linker → "link: extra operand").
- Run `npm run build` in `apps/desktop` once before the first `cargo build`
  (Tauri's `generate_context!` needs `apps/desktop/dist`).
- The app is a tray app: after launch, look for the tray icon; the settings
  window is opened from the tray. Stop a running instance with
  `Stop-Process -Name openflow-desktop -Force` before relaunching.

## Verifying it actually works

After launch, confirm the log shows `pipeline ready` and `opening input stream`.
For AI cleanup, Ollama must be running with the configured model pulled — see
the `ollama-setup` skill.
