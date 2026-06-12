# Build release binary and stage a portable installer folder + zip.
# Usage: .\scripts\build-installer.ps1

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $root

Write-Host "Building orchid-app (release)..."
cargo build --release -p orchid-app
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

$version = (Select-String -Path "Cargo.toml" -Pattern '^version = ' | ForEach-Object {
    $_.Line -replace 'version = "(.*)"', '$1'
} | Select-Object -First 1)
if (-not $version) { $version = "0.1.0" }

$distName = "Orchid-$version-win64"
$distDir = Join-Path $root "dist\$distName"
$releaseExe = Join-Path $root "target\release\orchid.exe"

New-Item -ItemType Directory -Force -Path $distDir | Out-Null
Copy-Item -Path $releaseExe -Destination (Join-Path $distDir "orchid.exe") -Force
Copy-Item -Path (Join-Path $root "scripts\install-desktop.ps1") -Destination (Join-Path $distDir "install.ps1") -Force
$iconIco = Join-Path $root "assets\logo\orchid-icon.ico"
if (Test-Path $iconIco) {
    Copy-Item -Path $iconIco -Destination (Join-Path $distDir "orchid-icon.ico") -Force
}
$logoSvg = Join-Path $root "assets\logo\orchid-logo.svg"
if (Test-Path $logoSvg) {
    Copy-Item -Path $logoSvg -Destination (Join-Path $distDir "orchid-logo.svg") -Force
}

$readme = @"
Orchid $version (Windows x64)

1. Run install.ps1 to install to %LOCALAPPDATA%\Programs\Orchid and add a Desktop shortcut.
2. Or run orchid.exe directly from this folder.

Built from the Orchid workspace.
"@
Set-Content -Path (Join-Path $distDir "README.txt") -Value $readme -Encoding UTF8

$zipPath = Join-Path $root "dist\$distName.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path $distDir -DestinationPath $zipPath -Force

Write-Host "Staged:  $distDir"
Write-Host "Package: $zipPath"
Pop-Location
