# One-click OpenFlow launcher: ensures Ollama is running, then starts the
# standalone OpenFlow app. No dev server or terminal needed.

$ErrorActionPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot

# 1. Ensure Ollama is up (AI cleanup + commands need it; dictation works without).
$code = curl.exe -s -o NUL -w "%{http_code}" http://localhost:11434/api/tags 2>$null
if ($code -ne "200") {
    $ollamaApp = "$env:LOCALAPPDATA\Programs\Ollama\ollama app.exe"
    $ollamaExe = "$env:LOCALAPPDATA\Programs\Ollama\ollama.exe"
    if (Test-Path $ollamaApp) { Start-Process $ollamaApp }
    elseif (Test-Path $ollamaExe) { Start-Process $ollamaExe -ArgumentList "serve" -WindowStyle Hidden }
}

# 2. Launch the standalone OpenFlow binary (prefer release if built, else debug).
$release = Join-Path $root "target\release\openflow-desktop.exe"
$debug = Join-Path $root "target\debug\openflow-desktop.exe"
$bin = if (Test-Path $release) { $release } elseif (Test-Path $debug) { $debug } else { $null }

if (-not $bin) {
    [System.Windows.Forms.MessageBox]::Show("OpenFlow is not built yet. Run: cargo build -p openflow-desktop") | Out-Null
    exit 1
}

# Avoid launching a second instance.
if (-not (Get-Process -Name "openflow-desktop" -ErrorAction SilentlyContinue)) {
    Start-Process $bin
}
