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

Write-Host "Installed: $installExe"
Write-Host "Shortcut:  $shortcutPath"
