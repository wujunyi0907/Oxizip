# Unregister Oxizip Sparse Package + classic ContextMenuHandlers.
param(
    [Parameter(Mandatory = $false)]
    [string]$InstallDir
)

$ErrorActionPreference = "Continue"

$pkg = Get-AppxPackage -Name "cn.wujunyi.oxizip.shellext" -ErrorAction SilentlyContinue
if ($pkg) {
    Remove-AppxPackage -Package $pkg.PackageFullName
    Write-Host "Removed sparse package $($pkg.PackageFullName)"
} else {
    Write-Host "Sparse package not installed"
}

if ($InstallDir) {
    $dll = Join-Path $InstallDir "oxizip-shellext.dll"
    if (Test-Path $dll) {
        $regsvr = Join-Path $env:SystemRoot "System32\regsvr32.exe"
        & $regsvr /s /u $dll
    }
}
