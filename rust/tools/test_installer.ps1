<#
.SYNOPSIS
  Tries the installer end to end: silent install into a scratch folder, checks
  what it put there, then uninstalls and checks nothing is left.

.DESCRIPTION
  Does not touch the real installation (it installs to a temporary directory
  and leaves %APPDATA%\Dockview alone). The CI runs it after building.

.PARAMETER Setup
  The installer to try (default: the newest dist\Dockview-Setup-*.exe).
#>
param([string]$Setup)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
if (-not $Setup) {
    $Setup = (Get-ChildItem (Join-Path $root "dist\Dockview-Setup-*.exe") | Sort-Object LastWriteTime | Select-Object -Last 1).FullName
}
if (-not $Setup -or -not (Test-Path $Setup)) { throw "No installer to try (run tools\build_installer.ps1)" }
$target = Join-Path ([IO.Path]::GetTempPath()) ("dockview-install-test-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
$log = "$target.log"
"installing $Setup -> $target"

$p = Start-Process $Setup -ArgumentList @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/DIR=`"$target`"", "/NOICONS", "/LOG=`"$log`"") -Wait -PassThru
if ($p.ExitCode -ne 0) { Get-Content $log -ErrorAction SilentlyContinue | Select-Object -Last 30; throw "the installer failed with code $($p.ExitCode)" }

$exe = Join-Path $target "Dockview.exe"
if (-not (Test-Path $exe)) { throw "Dockview.exe was not installed" }
$info = (Get-Item $exe).VersionInfo
"installed: $($info.ProductName) $($info.FileVersion)"
if ($info.ProductName -ne "Dockview") { throw "the program has no version information" }
if (-not (Test-Path (Join-Path $target "unins000.exe"))) { throw "there is no uninstaller" }
$uninstall = Get-ChildItem "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall" | Where-Object { $_.PSChildName -like "{26C04BD3-242C-49E2-A47B-9EB9D0FC3522}*" }
if (-not $uninstall) { throw "the uninstall entry is not in the registry" }

$u = Start-Process (Join-Path $target "unins000.exe") -ArgumentList @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART") -Wait -PassThru
if ($u.ExitCode -ne 0) { throw "the uninstaller failed with code $($u.ExitCode)" }
Start-Sleep -Seconds 2
if (Test-Path $exe) { throw "Dockview.exe is still there after uninstalling" }
if (Get-ChildItem "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall" | Where-Object { $_.PSChildName -like "{26C04BD3-242C-49E2-A47B-9EB9D0FC3522}*" }) { throw "the uninstall entry is still in the registry" }
Remove-Item $target -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $log -Force -ErrorAction SilentlyContinue
"the installer installs and uninstalls cleanly"
