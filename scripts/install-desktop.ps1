# Install Orchid from a release build and create a Desktop shortcut.
# Usage: .\scripts\install-desktop.ps1 [-SourceDir target\release]

param(
    [string] $SourceDir = (Join-Path $PSScriptRoot "..\target\release")
)

$ErrorActionPreference = "Stop"

$exeName = "orchid.exe"
$sourceExe = Join-Path $SourceDir $exeName
if (-not (Test-Path $sourceExe)) {
    Write-Error "Release binary not found: $sourceExe. Run: cargo build --release -p orchid-app"
}

$installRoot = Join-Path $env:LOCALAPPDATA "Programs\Orchid"
$installExe = Join-Path $installRoot $exeName
$iconSource = Join-Path $PSScriptRoot "..\assets\logo\orchid-icon.ico"
$installIcon = Join-Path $installRoot "orchid-icon.ico"

New-Item -ItemType Directory -Force -Path $installRoot | Out-Null

# Prefer a staged swap if a previous install couldn't overwrite a running exe.
$stagedExe = Join-Path $installRoot "orchid.exe.new"
try {
    Copy-Item -Path $sourceExe -Destination $installExe -Force
    if (Test-Path $stagedExe) { Remove-Item $stagedExe -Force }
} catch {
    Copy-Item -Path $sourceExe -Destination $stagedExe -Force
    Write-Warning "orchid.exe is in use. Close Orchid, then re-run this script (or rename orchid.exe.new -> orchid.exe)."
}

# Runtime companions staged next to orchid.exe by orchid-app/build.rs (pdfium, libmpv).
Get-ChildItem -Path $SourceDir -Filter "*.dll" -File -ErrorAction SilentlyContinue |
    ForEach-Object { Copy-Item -Path $_.FullName -Destination (Join-Path $installRoot $_.Name) -Force }

if (Test-Path $iconSource) {
    Copy-Item -Path $iconSource -Destination $installIcon -Force
}

$desktop = [Environment]::GetFolderPath("Desktop")
$shortcutPath = Join-Path $desktop "Orchid.lnk"

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $installExe
$shortcut.WorkingDirectory = $installRoot
$shortcut.Description = "Orchid desktop shell"
if (Test-Path $installIcon) {
    $shortcut.IconLocation = "$installIcon,0"
}
$shortcut.Save()

# Associate .orchid with Orchid (per-user HKCU; no admin required).
$progId = "Orchid.orchid"
$mime = "application/vnd.orchid"
$iconPath = if (Test-Path $installIcon) { $installIcon } else { $installExe }
$classes = "HKCU:\Software\Classes"
New-Item -Path "$classes\.orchid" -Force | Out-Null
Set-ItemProperty -Path "$classes\.orchid" -Name "(default)" -Value $progId
Set-ItemProperty -Path "$classes\.orchid" -Name "Content Type" -Value $mime
New-Item -Path "$classes\$progId" -Force | Out-Null
Set-ItemProperty -Path "$classes\$progId" -Name "(default)" -Value "Orchid Document"
New-Item -Path "$classes\$progId\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "$classes\$progId\DefaultIcon" -Name "(default)" -Value "`"$iconPath`",0"
New-Item -Path "$classes\$progId\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "$classes\$progId\shell\open\command" -Name "(default)" -Value "`"$installExe`" `"%1`""
New-Item -Path "$classes\Applications\$exeName\SupportedTypes" -Force | Out-Null
Set-ItemProperty -Path "$classes\Applications\$exeName\SupportedTypes" -Name ".orchid" -Value ""

Write-Host "Installed: $installExe"
Write-Host "Shortcut:  $shortcutPath"
Write-Host "Associated .orchid ($mime) with Orchid"
