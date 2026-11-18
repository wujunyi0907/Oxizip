# Build Oxizip custom installer (Oxizip-Setup.exe) with embedded payload.
# Prerequisites: Rust MSVC toolchain
#
# Output: dist\Oxizip-Setup-x64.exe
#   GUI binary name: oxizip.exe
#   CLI binary name: oxizip-cli.exe

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path

New-Item -ItemType Directory -Force -Path dist | Out-Null
$payloadRoot = Join-Path $PWD "target\setup-payload"
$payloadStage = Join-Path $payloadRoot "stage"
$payloadZip = Join-Path $payloadRoot "payload.zip"

Write-Host "==> Building release binaries..."
cargo build --release -p oxizip-cli -p oxizip-gui
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Staging setup payload..."
if (Test-Path $payloadRoot) { Remove-Item $payloadRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $payloadStage | Out-Null
Copy-Item "target\release\oxizip.exe" $payloadStage
Copy-Item "target\release\oxizip-cli.exe" $payloadStage
Copy-Item "LICENSE" $payloadStage
Copy-Item "README.md" $payloadStage
New-Item -ItemType Directory -Force -Path (Join-Path $payloadStage "assets\icons") | Out-Null
Copy-Item "crates\oxizip-gui\assets\icons\*.ico" (Join-Path $payloadStage "assets\icons")

Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path $payloadZip) { Remove-Item $payloadZip -Force }
[System.IO.Compression.ZipFile]::CreateFromDirectory($payloadStage, $payloadZip)
Write-Host "    payload: $payloadZip ($((Get-Item $payloadZip).Length) bytes)"

Write-Host "==> Building Oxizip-Setup (embeds payload)..."
cargo build --release -p oxizip-setup
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$built = Join-Path $PWD "target\release\Oxizip-Setup.exe"
$out = Join-Path $PWD "dist\Oxizip-Setup-x64.exe"
Copy-Item $built $out -Force

Write-Host ""
Write-Host "Done: $out"
Write-Host "This is the custom installer UI (path + Agree and Install)."
Write-Host "Run it to install into C:\Program Files\Oxizip by default."
