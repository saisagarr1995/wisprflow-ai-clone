# OpenFlow build helper. Sets the toolchain env explicitly (in case a fresh
# shell lacks it) and runs the requested step.
#
# Usage: scripts/build.ps1 [test|clippy|fmt|run|bundle|check]

param([string]$Task = "check")

$ErrorActionPreference = "Stop"

# --- toolchain env (whisper.cpp needs cmake + libclang; MSVC needs its bin) ---
$cmakeBin = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
if (Test-Path $cmakeBin) { $env:Path = "$env:Path;$cmakeBin" }
$llvm = "C:\Program Files\LLVM\bin"
if (Test-Path $llvm) { $env:LIBCLANG_PATH = $llvm }

$root = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
Set-Location $root

function Ensure-UI {
    if (-not (Test-Path "apps/desktop/dist")) {
        Push-Location "apps/desktop"
        if (-not (Test-Path "node_modules")) { npm install --no-audit --no-fund }
        npm run build
        Pop-Location
    }
}

switch ($Task) {
    "test"   { Ensure-UI; cargo test --workspace }
    "clippy" { cargo clippy --workspace --all-targets -- -D warnings }
    "fmt"    { cargo fmt --all }
    "check"  { Ensure-UI; cargo build --workspace }
    "run" {
        Stop-Process -Name "openflow-desktop" -Force -ErrorAction SilentlyContinue
        Push-Location "apps/desktop"
        if (-not (Test-Path "node_modules")) { npm install --no-audit --no-fund }
        npm run tauri dev
        Pop-Location
    }
    "bundle" {
        Push-Location "apps/desktop"
        if (-not (Test-Path "node_modules")) { npm install --no-audit --no-fund }
        npm run tauri build
        Pop-Location
    }
    default { Write-Error "Unknown task '$Task'. Use test|clippy|fmt|run|bundle|check." }
}
