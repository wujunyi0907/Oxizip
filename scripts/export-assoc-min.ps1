# Minimal file-association registry snapshot (desktop by default).
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\export-assoc-min.ps1 -Label bandizip-fresh
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\export-assoc-min.ps1 -Label oxizip-fresh

param(
    [string]$Label = "assoc-min",
    [string]$OutRoot = ""
)

$ErrorActionPreference = "Continue"
if (-not $OutRoot) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $desktop = [Environment]::GetFolderPath("Desktop")
    $OutRoot = Join-Path $desktop "$Label-$stamp"
}
$OutRoot = [System.IO.Path]::GetFullPath($OutRoot)
New-Item -ItemType Directory -Path $OutRoot -Force | Out-Null

function Test-RegKey([string]$RegPath) {
    # HKLM\Software\Foo -> Registry::HKEY_LOCAL_MACHINE\Software\Foo
    $p = $RegPath
    $p = $p -replace '^HKLM\\', 'HKEY_LOCAL_MACHINE\'
    $p = $p -replace '^HKCU\\', 'HKEY_CURRENT_USER\'
    $p = $p -replace '^HKCR\\', 'HKEY_CLASSES_ROOT\'
    Test-Path -LiteralPath ("Registry::$p")
}

function Export-Key([string]$RegPath, [string]$FileName, [bool]$Required = $false) {
    $dst = Join-Path $OutRoot $FileName
    if (-not (Test-RegKey $RegPath)) {
        if ($Required) {
            Write-Error "missing required key: $RegPath"
        } else {
            Write-Host "skip (missing) $RegPath"
        }
        return $false
    }
    # Avoid PowerShell treating reg.exe stderr as terminating error.
    $oldEap = $ErrorActionPreference
    $ErrorActionPreference = "SilentlyContinue"
    & reg.exe export $RegPath $dst /y 1>$null 2>$null
    $code = $LASTEXITCODE
    $ErrorActionPreference = $oldEap
    if ($code -ne 0 -or -not (Test-Path $dst) -or (Get-Item $dst).Length -lt 20) {
        Remove-Item $dst -ErrorAction SilentlyContinue
        if ($Required) {
            Write-Error "export failed: $RegPath (exit=$code)"
        } else {
            Write-Host "skip (export failed) $RegPath"
        }
        return $false
    }
    Write-Host "ok  $RegPath"
    return $true
}

# Minimal set
Export-Key "HKLM\Software\Classes" "01-HKLM-Classes.reg" -Required $true | Out-Null
Export-Key "HKCU\Software\Classes" "02-HKCU-Classes.reg" -Required $true | Out-Null
Export-Key "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts" "03-HKCU-FileExts.reg" -Required $true | Out-Null

# Optional extras
Export-Key "HKCU\Software\RegisteredApplications" "04-HKCU-RegisteredApplications.reg" | Out-Null
Export-Key "HKLM\Software\RegisteredApplications" "05-HKLM-RegisteredApplications.reg" | Out-Null
Export-Key "HKCU\Software\cn.wujunyi.oxizip" "06-HKCU-oxizip.reg" | Out-Null
Export-Key "HKLM\Software\cn.wujunyi.oxizip" "07-HKLM-oxizip.reg" | Out-Null
Export-Key "HKCU\Software\Bandizip" "08-HKCU-Bandizip.reg" | Out-Null
Export-Key "HKLM\Software\Bandizip" "09-HKLM-Bandizip.reg" | Out-Null

# Combined (UTF-8) for diff tools
$combined = Join-Path $OutRoot "ALL-COMBINED.reg"
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("Windows Registry Editor Version 5.00")
[void]$sb.AppendLine("; minimal assoc export  label=$Label  $(Get-Date -Format o)")
[void]$sb.AppendLine()
Get-ChildItem $OutRoot -Filter "*.reg" |
    Where-Object { $_.Name -ne "ALL-COMBINED.reg" } |
    Sort-Object Name |
    ForEach-Object {
        $raw = Get-Content -LiteralPath $_.FullName -Raw
        $body = $raw -replace "(?m)^Windows Registry Editor Version 5\.00\s*", ""
        [void]$sb.AppendLine("; --- $($_.Name) ---")
        [void]$sb.AppendLine($body.TrimEnd())
        [void]$sb.AppendLine()
    }
[System.IO.File]::WriteAllText($combined, $sb.ToString(), [System.Text.UTF8Encoding]::new($false))

Write-Host ""
Write-Host "OUT=$OutRoot"
Write-Host "COMBINED=$combined"
