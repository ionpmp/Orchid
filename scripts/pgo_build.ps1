# Profile-guided dist build for orchid-app (Windows).
#
#   rustup component add llvm-tools-preview
#   .\scripts\pgo_build.ps1 -Generate
#   # run target\dist\orchid.exe through a representative session, then:
#   .\scripts\pgo_build.ps1 -Use
#
# Optional: $env:RUSTFLAGS = "-C target-cpu=x86-64-v3" before either phase.

param(
    [switch]$Generate,
    [switch]$Use
)

$ErrorActionPreference = "Stop"
if (-not $Generate -and -not $Use) {
    Write-Host "Specify -Generate (instrumented build) or -Use (rebuild with merged profiles)."
    exit 1
}

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $root
try {
    $profDir = Join-Path $root "target\pgo-data"
    $merged = Join-Path $profDir "merged.profdata"
    $sysroot = (rustc --print sysroot).Trim()
    $profdata = Get-ChildItem -Path $sysroot -Recurse -Filter "llvm-profdata.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $profdata) {
        throw "llvm-profdata.exe not found. Run: rustup component add llvm-tools-preview"
    }

    $extra = $env:RUSTFLAGS
    if ($Generate) {
        New-Item -ItemType Directory -Force -Path $profDir | Out-Null
        $env:RUSTFLAGS = @("-Cprofile-generate=$profDir", $extra | Where-Object { $_ }) -join " "
        Write-Host "Building instrumented dist binary (RUSTFLAGS=$($env:RUSTFLAGS))..."
        cargo build --profile dist -p orchid-app
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Write-Host "Run target\dist\orchid.exe, then .\scripts\pgo_build.ps1 -Use"
    }

    if ($Use) {
        if (-not (Get-ChildItem -Path $profDir -Filter "*.profraw" -ErrorAction SilentlyContinue)) {
            throw "No .profraw files in $profDir. Run the instrumented binary first."
        }
        & $profdata merge -o $merged $profDir
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        $env:RUSTFLAGS = @("-Cprofile-use=$merged", $extra | Where-Object { $_ }) -join " "
        Write-Host "Building PGO-optimized dist binary (RUSTFLAGS=$($env:RUSTFLAGS))..."
        cargo build --profile dist -p orchid-app
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Write-Host "Optimized binary: target\dist\orchid.exe"
    }
} finally {
    Pop-Location
}
