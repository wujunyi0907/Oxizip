# Build and Authenticode-sign the Win11 Sparse MSIX (AllowExternalContent).
# Called from package-windows.ps1. Requires Windows SDK makeappx + signtool.
#
# Output under -OutDir:
#   AppxManifest.xml, Assets\StoreLogo.png, oxizip-shellext.msix, Oxizip.cer
#
# The private PFX is NOT written to OutDir (install machines only need the .cer).

param(
    [Parameter(Mandatory = $true)]
    [string]$OutDir,
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"

function Find-SdkTool([string]$Name) {
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    if (-not (Test-Path $kits)) { return $null }
    $hit = Get-ChildItem $kits -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object {
            $p = Join-Path $_.FullName "x64\$Name"
            if (Test-Path $p) { $p }
        } |
        Select-Object -First 1
    return $hit
}

function Write-Utf8NoBom([string]$Path, [string]$Text) {
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Text, $utf8)
}

$makeappx = Find-SdkTool "makeappx.exe"
$signtool = Find-SdkTool "signtool.exe"
if (-not $makeappx -or -not $signtool) {
    throw "Windows SDK makeappx/signtool not found under Program Files (x86)\Windows Kits\10\bin"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestSrc = Join-Path $repoRoot "packaging\windows\sparse\AppxManifest.xml"
if (-not (Test-Path $manifestSrc)) {
    throw "missing $manifestSrc"
}

# Identity Version must be a.b.c.d
$ver4 = if ($Version -match '^\d+\.\d+\.\d+\.\d+$') { $Version } else { "$Version.0" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$manifestText = Get-Content -Raw $manifestSrc
$manifestText = [regex]::Replace(
    $manifestText,
    '(<Identity\b[^>]*\bVersion=")[^"]+(")',
    { param($m) $m.Groups[1].Value + $ver4 + $m.Groups[2].Value }
)
$manifestText = [regex]::Replace(
    $manifestText,
    'MaxVersionTested="[^"]+"',
    'MaxVersionTested="10.0.26100.0"'
)

# Pack input must contain ONLY the sparse package layout (not the .msix/.cer outputs).
$packRoot = Join-Path $env:TEMP ("oxizip-sparse-pack-" + [guid]::NewGuid().ToString("n"))
$work = Join-Path $env:TEMP ("oxizip-sparse-sign-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path (Join-Path $packRoot "Assets") | Out-Null
New-Item -ItemType Directory -Force -Path $work | Out-Null

try {
    $packManifest = Join-Path $packRoot "AppxManifest.xml"
    Write-Utf8NoBom $packManifest $manifestText

    $pngB64 = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAALElEQVRYR+3QQREAAAABQfqXFsNuwAo4AAAAAElFTkSuQmCC"
    $packLogo = Join-Path $packRoot "Assets\StoreLogo.png"
    [IO.File]::WriteAllBytes($packLogo, [Convert]::FromBase64String($pngB64))

    # Ship the same layout for loose-register fallback.
    New-Item -ItemType Directory -Force -Path (Join-Path $OutDir "Assets") | Out-Null
    Copy-Item $packManifest (Join-Path $OutDir "AppxManifest.xml") -Force
    Copy-Item $packLogo (Join-Path $OutDir "Assets\StoreLogo.png") -Force

    $cert = Get-ChildItem Cert:\LocalMachine\My -CodeSigningCert -ErrorAction SilentlyContinue |
        Where-Object { $_.Subject -eq "CN=Oxizip" } |
        Select-Object -First 1
    if (-not $cert) {
        $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
            Where-Object { $_.Subject -eq "CN=Oxizip" } |
            Select-Object -First 1
    }
    if (-not $cert) {
        $cert = New-SelfSignedCertificate `
            -Type CodeSigningCert `
            -Subject "CN=Oxizip" `
            -CertStoreLocation "Cert:\CurrentUser\My" `
            -KeyExportPolicy Exportable `
            -NotAfter (Get-Date).AddYears(10)
        Write-Host "Created CN=Oxizip cert: $($cert.Thumbprint)"
    }

    $pfx = Join-Path $work "oxizip.pfx"
    $pfxPassPlain = "oxizip-sparse-" + [guid]::NewGuid().ToString("n")
    $pfxPass = ConvertTo-SecureString -String $pfxPassPlain -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pfxPass | Out-Null

    $cer = Join-Path $OutDir "Oxizip.cer"
    Export-Certificate -Cert $cert -FilePath $cer | Out-Null

    $msix = Join-Path $OutDir "oxizip-shellext.msix"
    if (Test-Path $msix) { Remove-Item $msix -Force }

    # /nv = sparse (do not require payload files inside the package).
    & $makeappx pack /d $packRoot /p $msix /nv
    if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }

    & $signtool sign /fd SHA256 /f $pfx /p $pfxPassPlain /v $msix
    if ($LASTEXITCODE -ne 0) { throw "signtool failed ($LASTEXITCODE)" }

    Write-Host "Signed sparse MSIX: $msix"
    Write-Host "Trust cert (public): $cer"
}
finally {
    Remove-Item $packRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
