# Single-process deployment for Windows.
# Builds frontend (Tailwind + trunk WASM), then starts the API which serves it.
# Usage: .\scripts\run.ps1

Set-Location "$PSScriptRoot\.."

# Load SECRETS.env into environment
if (Test-Path "SECRETS.env") {
    Get-Content "SECRETS.env" | ForEach-Object {
        if ($_ -match '^\s*([^#][^=]+)=(.+)$') {
            $key   = $Matches[1].Trim()
            $value = $Matches[2].Trim()
            [System.Environment]::SetEnvironmentVariable($key, $value, "Process")
        }
    }
    Write-Host "Loaded SECRETS.env" -ForegroundColor Cyan
} else {
    Write-Warning "SECRETS.env not found — API keys will be missing"
}

# Build Tailwind CSS
Push-Location "crates\frontend"
Write-Host "Building Tailwind CSS..." -ForegroundColor Cyan
npm install --silent
npm run build:css
Pop-Location

# Build WASM frontend
Write-Host "Building frontend (trunk)..." -ForegroundColor Cyan
trunk build --release

# Build API
Write-Host "Building API..." -ForegroundColor Cyan
cargo build --release -p gem-finder-api

# Run — API serves frontend from dist/
Write-Host "Starting Gem Finder on http://localhost:3000" -ForegroundColor Green
$env:SERVE_FRONTEND = "1"
.\target\release\gem-finder-api.exe
