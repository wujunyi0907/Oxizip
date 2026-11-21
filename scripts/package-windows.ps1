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

$CargoTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PWD "target" }
$ReleaseDir = Join-Path $CargoTarget "release"
Write-Host "Cargo target: $CargoTarget"

New-Item -ItemType Directory -Force -Path dist | Out-Null
$payloadRoot = Join-Path $CargoTarget "setup-payload"
$payloadStage = Join-Path $payloadRoot "stage"
$payloadZip = Join-Path $payloadRoot "payload.zip"

Write-Host "==> Building release binaries..."
cargo build --release -p oxizip-cli -p oxizip-gui -p oxizip-shellext
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Staging setup payload..."
if (Test-Path $payloadRoot) { Remove-Item $payloadRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $payloadStage | Out-Null
Copy-Item (Join-Path $ReleaseDir "oxizip.exe") $payloadStage
Copy-Item (Join-Path $ReleaseDir "oxizip-cli.exe") $payloadStage
$shellextDll = Join-Path $ReleaseDir "oxizip_shellext.dll"
if (-not (Test-Path $shellextDll)) {
    throw "missing $shellextDll — build oxizip-shellext"
}
Copy-Item $shellextDll (Join-Path $payloadStage "oxizip-shellext.dll")
Copy-Item "LICENSE" $payloadStage
Copy-Item "README.md" $payloadStage

# Win11 Sparse Package: signed MSIX + public .cer + loose manifest fallback.
$sparseStage = Join-Path $payloadStage "sparse"
Write-Host "    building signed sparse MSIX..."
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PWD "scripts\build-sparse-msix.ps1") `
    -OutDir $sparseStage -Version $Version
if ($LASTEXITCODE -ne 0) { throw "build-sparse-msix.ps1 failed" }
foreach ($req in @(
        (Join-Path $sparseStage "oxizip-shellext.msix"),
        (Join-Path $sparseStage "Oxizip.cer"),
        (Join-Path $sparseStage "AppxManifest.xml")
    )) {
    if (-not (Test-Path $req)) { throw "missing sparse artifact $req" }
}
$scriptsStage = Join-Path $payloadStage "scripts"
New-Item -ItemType Directory -Force -Path $scriptsStage | Out-Null
Copy-Item "scripts\register-sparse.ps1" $scriptsStage
Copy-Item "scripts\unregister-sparse.ps1" $scriptsStage
Write-Host "    bundled oxizip-shellext.dll + signed sparse MSIX"
New-Item -ItemType Directory -Force -Path (Join-Path $payloadStage "assets\icons") | Out-Null
# Installed product icons only — exclude setup-only / shell-uac composites.
$excludeIcons = @("oxizip-uac.ico", "oxizip-setup.ico")
Get-ChildItem "crates\oxizip-gui\assets\icons\*.ico" |
    Where-Object { $excludeIcons -notcontains $_.Name } |
    ForEach-Object { Copy-Item $_.FullName (Join-Path $payloadStage "assets\icons") }

# Official UnRAR / wimlib CLIs under repo `lib/<name>/` → `<app>\lib\<name>\`.
# `lib/zpaq` is source only (embedded at build time); do not ship it in the installer.
$libSrc = Join-Path $PWD "lib"
$unrarSrc = Join-Path $libSrc "unrar"
$wimlibSrc = Join-Path $libSrc "wimlib"
foreach ($req in @(
        (Join-Path $unrarSrc "UnRAR.exe"),
        (Join-Path $wimlibSrc "wimlib-imagex.exe"),
        (Join-Path $wimlibSrc "libwim-15.dll")
    )) {
    if (-not (Test-Path $req)) {
        throw "missing $req — keep official redistributables under lib/unrar and lib/wimlib"
    }
}
$libDst = Join-Path $payloadStage "lib"
New-Item -ItemType Directory -Force -Path (Join-Path $libDst "unrar") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $libDst "wimlib") | Out-Null
Copy-Item (Join-Path $unrarSrc "*") (Join-Path $libDst "unrar") -Force
Copy-Item (Join-Path $wimlibSrc "*") (Join-Path $libDst "wimlib") -Force
Write-Host "    bundled lib/unrar + lib/wimlib"

Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path $payloadZip) { Remove-Item $payloadZip -Force }
[System.IO.Compression.ZipFile]::CreateFromDirectory($payloadStage, $payloadZip)
Write-Host "    payload: $payloadZip ($((Get-Item $payloadZip).Length) bytes)"

Write-Host "==> Building Oxizip-Setup (embeds payload)..."
cargo build --release -p oxizip-setup
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$built = Join-Path $ReleaseDir "Oxizip-Setup.exe"
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
