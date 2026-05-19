# NexusMind — Reset Demo Data (Windows PowerShell)
# Usage: .\scripts\reset-demo.ps1

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BackendDir = Join-Path $ScriptDir "..\apps\backend"

Write-Host "Building seed binary..."
Push-Location $BackendDir
cargo build --release --bin nexusmind-seed
if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
Pop-Location

Write-Host "Resetting demo data..."
New-Item -ItemType Directory -Force -Path "$BackendDir\data" | Out-Null
& "$BackendDir\target\release\nexusmind-seed.exe"
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host ""
Write-Host "Demo data ready. Start the server with:"
Write-Host "  cargo run --manifest-path $BackendDir\Cargo.toml"
Write-Host "  Open http://localhost:8080/v1/health"
