# Build Todoing release artifacts for the current Windows host.
#
# One command produces, under release\dist\<VERSION>\windows-<ARCH>\:
#   - the plain release binary (todoing-<VERSION>-windows-<ARCH>.exe)
#   - the portable ZIP package
#   - the .sha256 files
#   - the release manifest JSON
# and then verifies them with check-artifacts.sh and verify-manifest.sh (via
# Git Bash, if available) or an inline PowerShell check.
#
# This script never tags, publishes, or pushes. See RELEASE.md.
param(
  [string]$Notes = "Release build.",
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Fail([string]$msg) { Write-Error "FAIL: $msg"; exit 1 }

# --- Version / provenance -------------------------------------------------
$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') { $Version = $Matches[1] } else { Fail "cannot determine version from Cargo.toml" }
$Commit = & git rev-parse HEAD 2>$null
if (-not $Commit) { Fail "not a git checkout (need source commit)" }
$Branch = & git rev-parse --abbrev-ref HEAD 2>$null
if (-not $Branch) { $Branch = "unknown" }
$Created = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")

# --- Platform / arch ------------------------------------------------------
$Platform = "windows"
$machine = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
switch ($machine) {
  "X64" { $Arch = "x64" }
  "Arm64" { $Arch = "arm64" }
  default { Fail "unsupported arch: $machine" }
}
$PlatformArch = "$Platform-$Arch"
$Exe = ".exe"

# --- Build ----------------------------------------------------------------
if (-not $SkipBuild) {
  Write-Host ">> cargo build --release"
  cargo build --release
  if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
}
$SrcBin = "target/release/todoing.exe"
if (-not (Test-Path $SrcBin)) { Fail "build output not found: $SrcBin" }

# --- Output layout --------------------------------------------------------
$Out = "release/dist/$Version/$PlatformArch"
$Stage = "release/dist/$Version/.stage-$PlatformArch"
if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
if (Test-Path $Stage) { Remove-Item $Stage -Recurse -Force }
New-Item -ItemType Directory -Path $Out -Force | Out-Null

$BinName = "todoing-$Version-$PlatformArch$Exe"
$PortableName = "todoing-$Version-$PlatformArch-portable.zip"
$Sha256Name = "todoing-$Version-$PlatformArch.sha256"
$PortableSha256Name = "todoing-$Version-$PlatformArch-portable.sha256"
$ManifestName = "todoing-$Version-$PlatformArch-release-manifest.json"

# --- Plain binary ---------------------------------------------------------
Copy-Item $SrcBin (Join-Path $Out $BinName)
$BinSha256 = (Get-FileHash (Join-Path $Out $BinName) -Algorithm SHA256).Hash.ToLower()

# --- Portable package -----------------------------------------------------
$PortableDir = Join-Path $Stage "todoing-$Version-$PlatformArch-portable"
New-Item -ItemType Directory -Path $PortableDir -Force | Out-Null
Copy-Item $SrcBin (Join-Path $PortableDir "todoing$Exe")

$versionTxt = @"
Todoing $Version
platform: $PlatformArch
source_commit: $Commit
source_branch: $Branch
binary: todoing$Exe
binary_sha256: $BinSha256
built: $Created
"@
Set-Content -Path (Join-Path $PortableDir "VERSION.txt") -Value $versionTxt -Encoding UTF8

$readme = @"
# Todoing $Version (portable, $PlatformArch)

A self-contained Todoing build. No installer, no admin rights, no Rust, no
Python, and no PATH changes are required.

## Run it

Extract this archive into any directory, then run the binary directly:

    .\todoing.exe --help

To use it from a project without installing anything, point the project at the
binary with the TODOING_BIN environment variable:

    set TODOING_BIN=C:\path\to\this\folder\todoing.exe

## Provenance

- Version: $Version
- Source commit: $Commit
- Binary SHA256: $BinSha256

Verify the binary before first use:

    certutil -hashfile todoing.exe SHA256

The package contains no task store and no configuration. A project's
.todoing\ store is created by the project on first use, not shipped here.
"@
Set-Content -Path (Join-Path $PortableDir "README.md") -Value $readme -Encoding UTF8

# --- Create the ZIP -------------------------------------------------------
$ZipPath = Join-Path $Out $PortableName
Compress-Archive -Path (Join-Path $PortableDir "*") -DestinationPath $ZipPath -Force
$PortableSha256 = (Get-FileHash $ZipPath -Algorithm SHA256).Hash.ToLower()

# --- SHA256 files ---------------------------------------------------------
Set-Content -Path (Join-Path $Out $Sha256Name) -Value ("$BinSha256  $BinName") -Encoding ASCII
Set-Content -Path (Join-Path $Out $PortableSha256Name) -Value ("$PortableSha256  $PortableName") -Encoding ASCII

# --- Release manifest -----------------------------------------------------
$manifest = [ordered]@{
  format_version = 1
  version = $Version
  source_commit = $Commit
  source_branch = $Branch
  created = $Created
  notes = $Notes
  platform = $Platform
  arch = $Arch
  binary = $BinName
  binary_sha256 = $BinSha256
  portable = $PortableName
  portable_sha256 = $PortableSha256
  sha256_file = $Sha256Name
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $Out $ManifestName) -Encoding UTF8

Remove-Item $Stage -Recurse -Force

Write-Host ""
Write-Host ">> artifacts in ${Out}:"
Get-ChildItem $Out -Name
Write-Host ""
Write-Host ">> running checks"
$bash = Get-Command bash -ErrorAction SilentlyContinue
if ($bash) {
  & bash "release/check-artifacts.sh" $Out
  if ($LASTEXITCODE -ne 0) { Fail "check-artifacts.sh failed" }
  & bash "release/verify-manifest.sh" $Out
  if ($LASTEXITCODE -ne 0) { Fail "verify-manifest.sh failed" }
} else {
  # Inline fallback check: every manifest-named file exists and hashes match.
  $m = Get-Content (Join-Path $Out $ManifestName) -Raw | ConvertFrom-Json
  foreach ($pair in @(@("binary", $m.binary, $m.binary_sha256), @("portable", $m.portable, $m.portable_sha256))) {
    $p = Join-Path $Out $pair[1]
    if (-not (Test-Path $p)) { Fail "missing artifact: $($pair[1])" }
    $actual = (Get-FileHash $p -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $pair[2].ToLower()) { Fail "sha256 mismatch for $($pair[1])" }
  }
  Write-Host "PASS: inline artifact check"
}
Write-Host ""
Write-Host "OK: release artifacts built and verified for $Version ($PlatformArch)"