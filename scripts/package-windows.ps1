# Build release binaries and produce an NSIS installer (.exe) via cargo-packager.
# Prerequisites: Rust toolchain; once: cargo install cargo-packager --locked
# Optional: NSIS is downloaded by cargo-packager on first run.

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

cargo build --release -p oxizip-cli -p oxizip-gui
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Skip before-packaging-command (already built); emit NSIS only.
cargo packager --release --formats nsis --binaries-dir target/release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Installer(s) under: dist\"
Get-ChildItem dist -Filter *.exe | ForEach-Object { $_.FullName }
