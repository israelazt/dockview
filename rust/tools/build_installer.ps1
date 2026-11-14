<#
.SYNOPSIS
  Builds the Dockview installer and the portable download (block "installable").

.DESCRIPTION
  Builds the release program (unless -SkipBuild), then:
    dist\Dockview-Setup-<version>.exe   the installer (Inno Setup 6)
    dist\Dockview-<version>-portable.zip   Dockview.exe alone, no install
    dist\*.sha256                         a hash file for each
  The version is the one in Cargo.toml. Development is unchanged: this only
  packages; `cargo run` and the plain `target\release\dockview.exe` still work.

  Needs Inno Setup 6 (https://jrsoftware.org/isdl.php, or
  `winget install JRSoftware.InnoSetup`). GitHub's Windows runners have it.

.PARAMETER SkipBuild
  Use the release build that is already in target\release.

.PARAMETER Sign
  Sign the installer with signtool, using the certificate named by the
  DOCKVIEW_SIGN_PFX and DOCKVIEW_SIGN_PASSWORD environment variables. Skipped
  when they are not set (an unsigned installer makes SmartScreen warn).
#>
param(
    [switch]$SkipBuild,
    [switch]$Sign
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

$version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { throw "Could not read the version from Cargo.toml" }
"Dockview $version"

if (-not $SkipBuild) {
    cargo build --release -p dockview
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
$exe = Join-Path $root "target\release\dockview.exe"
if (-not (Test-Path $exe)) { throw "Missing $exe (build it first, or run without -SkipBuild)" }

$candidates = @(
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
)
$iscc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    $found = Get-Command iscc -ErrorAction SilentlyContinue
    if ($found) { $iscc = $found.Source }
}
if (-not $iscc) { throw "Inno Setup 6 not found. Install it (winget install JRSoftware.InnoSetup) and run again." }

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
Remove-Item (Join-Path $dist "Dockview-*") -Force -ErrorAction SilentlyContinue

& $iscc "/DAppVersion=$version" "/DSourceExe=$exe" "/DOutputDir=$dist" (Join-Path $root "installer\dockview.iss")
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
$setup = Join-Path $dist "Dockview-Setup-$version.exe"

if ($Sign) {
    if (-not $env:DOCKVIEW_SIGN_PFX) { Write-Warning "DOCKVIEW_SIGN_PFX is not set: the installer stays unsigned." }
    else {
        $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue | Sort-Object FullName | Select-Object -Last 1
        if (-not $signtool) { throw "signtool.exe not found (install the Windows SDK)" }
        & $signtool.FullName sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f $env:DOCKVIEW_SIGN_PFX /p $env:DOCKVIEW_SIGN_PASSWORD $setup
        if ($LASTEXITCODE -ne 0) { throw "signtool failed" }
    }
}

# The portable download: the same program, nothing to install.
$stage = Join-Path $dist "portable"
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe (Join-Path $stage "Dockview.exe")
$zip = Join-Path $dist "Dockview-$version-portable.zip"
Compress-Archive -Path (Join-Path $stage "Dockview.exe") -DestinationPath $zip -Force
Remove-Item $stage -Recurse -Force

foreach ($file in @($setup, $zip)) {
    $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
    "$hash  $(Split-Path $file -Leaf)" | Set-Content -Encoding ascii "$file.sha256"
}
Get-ChildItem $dist | Format-Table Name, Length
