# Register Oxizip Win11 Sparse Package (FileExplorerContextMenus).
# Requires elevation. ExternalLocation = install dir containing oxizip-shellext.dll.
#
# Preferred: signed sparse\oxizip-shellext.msix + sparse\Oxizip.cer
# Fallback: development unlock + loose AppxManifest -Register
#
# Do NOT call regsvr32 — loading a freshly installed shell DLL under AV often
# hangs the elevated installer (CREATE_NO_WINDOW) after the first log line.
#
# Usage:
#   .\scripts\register-sparse.ps1 -InstallDir "C:\Program Files\Oxizip"

param(
    [Parameter(Mandatory = $true)]
    [string]$InstallDir,
    [int]$TimeoutSec = 60
)

$ErrorActionPreference = "Stop"
$InstallDir = (Resolve-Path $InstallDir).Path
$logPath = Join-Path $InstallDir "sparse-register.log"

function Write-Log([string]$Message) {
    $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message
    try { Add-Content -Path $logPath -Value $line -Encoding UTF8 -ErrorAction SilentlyContinue } catch {}
    Write-Host $line
}

try {
    Set-Content -Path $logPath -Value "" -Encoding UTF8
} catch {
    $logPath = Join-Path $env:TEMP "oxizip-sparse-register.log"
    Set-Content -Path $logPath -Value "" -Encoding UTF8
}

Write-Log "InstallDir=$InstallDir"
Write-Log "TimeoutSec=$TimeoutSec pid=$PID"

$dll = Join-Path $InstallDir "oxizip-shellext.dll"
Write-Log "Checking DLL..."
if (-not (Test-Path -LiteralPath $dll)) {
    Write-Log "ERROR: oxizip-shellext.dll not found"
    Write-Error "oxizip-shellext.dll not found in $InstallDir"
}
Write-Log "DLL ok"

$sparseRoot = Join-Path $InstallDir "sparse"
$msix = Join-Path $sparseRoot "oxizip-shellext.msix"
$cer = Join-Path $sparseRoot "Oxizip.cer"
$manifestPath = Join-Path $sparseRoot "AppxManifest.xml"
Write-Log "msix=$(Test-Path -LiteralPath $msix) cer=$(Test-Path -LiteralPath $cer) manifest=$(Test-Path -LiteralPath $manifestPath)"

if (-not (Test-Path -LiteralPath $manifestPath)) {
    $repoManifest = Join-Path $PSScriptRoot "..\packaging\windows\sparse\AppxManifest.xml"
    if (Test-Path -LiteralPath $repoManifest) {
        New-Item -ItemType Directory -Force -Path (Join-Path $sparseRoot "Assets") | Out-Null
        Copy-Item $repoManifest $manifestPath -Force
        Write-Log "Copied repo AppxManifest.xml"
    }
}
if (-not (Test-Path -LiteralPath $manifestPath) -and -not (Test-Path -LiteralPath $msix)) {
    Write-Log "ERROR: sparse package files missing"
    Write-Error "Sparse package files not found under $sparseRoot"
}

$logo = Join-Path $sparseRoot "Assets\StoreLogo.png"
if (-not (Test-Path -LiteralPath $logo)) {
    New-Item -ItemType Directory -Force -Path (Join-Path $sparseRoot "Assets") | Out-Null
    $pngB64 = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAALElEQVRYR+3QQREAAAABQfqXFsNuwAo4AAAAAElFTkSuQmCC"
    [IO.File]::WriteAllBytes($logo, [Convert]::FromBase64String($pngB64))
}

function Import-TrustCer([string]$CerPath) {
    if (-not (Test-Path -LiteralPath $CerPath)) {
        Write-Log "No .cer to import"
        return
    }
    foreach ($store in @(
            "Cert:\LocalMachine\TrustedPeople",
            "Cert:\CurrentUser\TrustedPeople"
        )) {
        try {
            $imported = Import-Certificate -FilePath $CerPath -CertStoreLocation $store -ErrorAction Stop
            Write-Log "Trusted publisher in $store thumbprint=$($imported.Thumbprint)"
        } catch {
            Write-Log "WARN: Import-Certificate $store failed: $_"
        }
    }
}

function Ensure-AppModelUnlock {
    param([switch]$AllowDev)
    $key = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock"
    try {
        New-Item -Path $key -Force | Out-Null
        Set-ItemProperty -Path $key -Name "AllowAllTrustedApps" -Value 1 -Type DWord -Force
        Write-Log "AllowAllTrustedApps=1"
        if ($AllowDev) {
            Set-ItemProperty -Path $key -Name "AllowDevelopmentWithoutDevLicense" -Value 1 -Type DWord -Force
            Write-Log "AllowDevelopmentWithoutDevLicense=1"
        }
    } catch {
        Write-Log "WARN: AppModelUnlock: $_"
    }
}

# Run Add-AppxPackage in-process via a nested PowerShell runspace with a wall-clock timeout.
# (Start-Job uses a separate process and often breaks elevated AppX registration.)
function Invoke-AddAppxWithTimeout {
    param(
        [string]$Path,
        [string]$ExternalLocation,
        [switch]$Register,
        [string]$Label,
        [int]$Seconds
    )
    Write-Log "BEGIN $Label (timeout ${Seconds}s)"
    $rs = [runspacefactory]::CreateRunspace()
    $rs.Open()
    $rs.SessionStateProxy.SetVariable("Path", $Path)
    $rs.SessionStateProxy.SetVariable("ExternalLocation", $ExternalLocation)
    $ps = [powershell]::Create()
    $ps.Runspace = $rs
    if ($Register) {
        [void]$ps.AddScript("Add-AppxPackage -Path `$Path -Register -ExternalLocation `$ExternalLocation -ErrorAction Stop; 'ok'")
    } else {
        [void]$ps.AddScript("Add-AppxPackage -Path `$Path -ExternalLocation `$ExternalLocation -ErrorAction Stop; 'ok'")
    }
    try {
        $iar = $ps.BeginInvoke()
        if (-not $iar.AsyncWaitHandle.WaitOne([int]($Seconds * 1000))) {
            Write-Log "TIMEOUT $Label — stopping"
            try { $ps.Stop() } catch {}
            return @{ Ok = $false; Error = "$Label timeout after ${Seconds}s" }
        }
        $result = $ps.EndInvoke($iar)
        if ($ps.HadErrors) {
            $err = ($ps.Streams.Error | ForEach-Object { "$_" }) -join "; "
            Write-Log "FAIL $Label : $err"
            return @{ Ok = $false; Error = $err }
        }
        Write-Log "OK $Label ($result)"
        return @{ Ok = $true; Error = $null }
    } catch {
        Write-Log "FAIL $Label : $_"
        return @{ Ok = $false; Error = "$_" }
    } finally {
        $ps.Dispose()
        $rs.Close()
        $rs.Dispose()
    }
}

# Registry-only scrub (never load oxizip-shellext.dll).
Write-Log "Scrubbing classic ContextMenuHandlers (registry only)..."
foreach ($hive in @("HKCU:\Software\Classes", "HKLM:\Software\Classes")) {
    foreach ($rel in @(
            "*\shellex\ContextMenuHandlers\OxizipCtx",
            "Directory\shellex\ContextMenuHandlers\OxizipCtx",
            "Directory\Background\shellex\ContextMenuHandlers\OxizipCtx"
        )) {
        $p = "$hive\$rel"
        if (Test-Path -LiteralPath $p) {
            Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue
            Write-Log "Removed $p"
        }
    }
}
Write-Log "Registry scrub done"

Write-Log "Querying existing Appx package..."
$existing = $null
try {
    $existing = Get-AppxPackage -Name "cn.wujunyi.oxizip.shellext" -ErrorAction SilentlyContinue
} catch {
    Write-Log "WARN: Get-AppxPackage: $_"
}
if ($existing) {
    Write-Log "Removing existing $($existing.PackageFullName)"
    try {
        Remove-AppxPackage -Package $existing.PackageFullName -ErrorAction Stop
        Write-Log "Removed existing package"
    } catch {
        Write-Log "WARN: Remove-AppxPackage: $_"
    }
} else {
    Write-Log "No existing package"
}

$registered = $false
$lastErr = $null

if (Test-Path -LiteralPath $msix) {
    Write-Log "Trying signed MSIX: $msix"
    Ensure-AppModelUnlock
    Import-TrustCer $cer
    $r = Invoke-AddAppxWithTimeout -Path $msix -ExternalLocation $InstallDir -Label "Add-AppxPackage MSIX" -Seconds $TimeoutSec
    if ($r.Ok) { $registered = $true } else { $lastErr = $r.Error }
}

if (-not $registered -and (Test-Path -LiteralPath $manifestPath)) {
    Write-Log "Trying loose Register: $manifestPath"
    Ensure-AppModelUnlock -AllowDev
    Import-TrustCer $cer
    $r = Invoke-AddAppxWithTimeout -Path $manifestPath -ExternalLocation $InstallDir -Register -Label "Add-AppxPackage -Register" -Seconds $TimeoutSec
    if ($r.Ok) { $registered = $true } else { $lastErr = $r.Error }
}

try {
    Add-Type -Namespace OxizipSparse -Name Native -MemberDefinition @"
        [System.Runtime.InteropServices.DllImport("shell32.dll")]
        public static extern void SHChangeNotify(int wEventId, uint uFlags, System.IntPtr dwItem1, System.IntPtr dwItem2);
"@ -ErrorAction SilentlyContinue
    [OxizipSparse.Native]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
} catch {}

Write-Log "Verifying Get-AppxPackage..."
$pkg = Get-AppxPackage -Name "cn.wujunyi.oxizip.shellext" -ErrorAction SilentlyContinue
if (-not $pkg) {
    Write-Log "ERROR: package not present. lastErr=$lastErr"
    Write-Error "Sparse package registration failed. See $logPath — last error: $lastErr"
}
Write-Log "OK: $($pkg.PackageFullName)"
Write-Host "Log: $logPath"
