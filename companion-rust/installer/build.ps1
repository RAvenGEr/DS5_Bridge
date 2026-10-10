<#
.SYNOPSIS
  Builds the DS5 Bridge Companion release binary and packages it into a
  Windows NSIS installer.

.DESCRIPTION
  1. cargo build --release (unless -SkipBuild)
  2. makensis -> artifacts\installer\DS5-Bridge-Companion-Setup-<version>.exe

  Requires: cargo (Rust MSVC toolchain) and NSIS 3.x on PATH
  (e.g. `scoop install nsis`).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File installer\build.ps1
  powershell -ExecutionPolicy Bypass -File installer\build.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
    # Package the existing target\release binary without rebuilding.
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$crateDir = Split-Path -Parent $PSScriptRoot   # companion-rust\
$nsi      = Join-Path $PSScriptRoot 'nsis\companion.nsi'

# Version from Cargo.toml (single source of truth).
$cargoToml = Join-Path $crateDir 'Cargo.toml'
$match = Select-String -Path $cargoToml -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1
if (-not $match) { throw "Could not read version from $cargoToml" }
$version = $match.Matches[0].Groups[1].Value

# VERSIONINFO needs a four-part version: 0.1.0 -> 0.1.0.0
$parts = @($version -split '\.')
while ($parts.Count -lt 4) { $parts += '0' }
$versionQuad = ($parts[0..3]) -join '.'

if (-not $SkipBuild) {
    Write-Host "==> cargo build --release ($version)"
    Push-Location $crateDir
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

$releaseExe = Join-Path $crateDir 'target\release\companion-rust.exe'
if (-not (Test-Path $releaseExe)) { throw "Missing $releaseExe - build first." }

$makensis = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $makensis) {
    throw 'makensis not found on PATH. Install NSIS 3.x, e.g.: scoop install nsis'
}

$outDir = Join-Path $crateDir 'artifacts\installer'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$outFile = Join-Path $outDir "DS5-Bridge-Companion-Setup-$version.exe"

Write-Host "==> makensis ($version -> $outFile)"
Push-Location (Split-Path -Parent $nsi)
try {
    & $makensis.Source `
        "/DPRODUCT_VERSION=$version" `
        "/DPRODUCT_VERSION_QUAD=$versionQuad" `
        "/DRELEASE_EXE=$releaseExe" `
        "/DOUTFILE=$outFile" `
        'companion.nsi'
    if ($LASTEXITCODE -ne 0) { throw "makensis failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

$size = [math]::Round((Get-Item $outFile).Length / 1MB, 2)
Write-Host "==> Done: $outFile ($size MB)"
