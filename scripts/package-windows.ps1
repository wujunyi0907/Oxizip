# Build Oxizip custom installer (Oxizip-Setup.exe) with embedded payload.
# Prerequisites: Rust MSVC toolchain
#
# Output: dist\Oxizip-Setup-<version>-x64.exe
#   GUI binary name: oxizip.exe
#   CLI binary name: oxizip-cli.exe

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path

function Get-WorkspaceVersion {
    $cargoToml = Get-Content -Raw (Join-Path $PWD "Cargo.toml")
    if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    throw "Cannot read workspace version from Cargo.toml"
}

$Version = Get-WorkspaceVersion
Write-Host "Workspace version: $Version"

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
# Installed product icons only — exclude setup-only / shell-uac composites.
$excludeIcons = @("oxizip-uac.ico", "oxizip-setup.ico")
Get-ChildItem "crates\oxizip-gui\assets\icons\*.ico" |
    Where-Object { $excludeIcons -notcontains $_.Name } |
    ForEach-Object { Copy-Item $_.FullName (Join-Path $payloadStage "assets\icons") }

Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path $payloadZip) { Remove-Item $payloadZip -Force }
[System.IO.Compression.ZipFile]::CreateFromDirectory($payloadStage, $payloadZip)
Write-Host "    payload: $payloadZip ($((Get-Item $payloadZip).Length) bytes)"

Write-Host "==> Building Oxizip-Setup (embeds payload)..."
cargo build --release -p oxizip-setup
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$built = Join-Path $PWD "target\release\Oxizip-Setup.exe"
$outName = "Oxizip-Setup-$Version-x64.exe"
$out = Join-Path $PWD "dist\$outName"
# Drop stale unversioned / other-version installers so CI uploads stay clean.
Get-ChildItem (Join-Path $PWD "dist") -Filter "Oxizip-Setup-*-x64.exe" -ErrorAction SilentlyContinue |
    Remove-Item -Force
Copy-Item $built $out -Force

Write-Host ""
Write-Host "Done: $out"
Write-Host "This is the custom installer UI (path + Agree and Install)."
Write-Host "Run it to install into C:\Program Files\Oxizip by default."
