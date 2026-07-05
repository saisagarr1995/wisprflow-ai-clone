# Ensure Ollama is installed, running, and has OpenFlow's model pulled.
# Reads the model tag from %APPDATA%\openflow\config.json (falls back to qwen3:4b).

$ErrorActionPreference = "Stop"

function Ollama-Up {
    $code = curl.exe -s -o NUL -w "%{http_code}" http://localhost:11434/api/tags 2>$null
    return $code -eq "200"
}

$exe = "$env:LOCALAPPDATA\Programs\Ollama\ollama.exe"

# 1. Install if missing.
if (-not (Test-Path $exe)) {
    Write-Host "Installing Ollama..."
    winget install --id Ollama.Ollama --accept-package-agreements --accept-source-agreements --silent
}

# 2. Start the server if not up.
if (-not (Ollama-Up)) {
    Write-Host "Starting Ollama..."
    $app = "$env:LOCALAPPDATA\Programs\Ollama\ollama app.exe"
    if (Test-Path $app) { Start-Process $app } else { Start-Process $exe -ArgumentList "serve" -WindowStyle Hidden }
    for ($i = 0; $i -lt 20 -and -not (Ollama-Up); $i++) { Start-Sleep 2 }
}
if (-not (Ollama-Up)) { Write-Error "Ollama did not come up."; exit 1 }
Write-Host "Ollama is running."

# 3. Determine the model from config, pull if absent.
$cfg = "$env:APPDATA\openflow\config.json"
$model = "qwen3:4b"
if (Test-Path $cfg) {
    try { $model = (Get-Content $cfg -Raw | ConvertFrom-Json).llm.cleanup_model } catch {}
}
$have = & $exe list 2>$null | Select-String ([regex]::Escape($model))
if (-not $have) {
    Write-Host "Pulling $model (this can take a few minutes)..."
    & $exe pull $model
} else {
    Write-Host "$model already present."
}
Write-Host "Done. OpenFlow AI cleanup is ready."
