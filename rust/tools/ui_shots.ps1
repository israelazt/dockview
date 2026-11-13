<#
.SYNOPSIS
  Renders the control pill and every popover in two looks (dark/English and
  light/Spanish) with the release build, as PNGs.
.DESCRIPTION
  Used to check that a change to the interface did not move a pixel: take the
  shots before and after, then compare them with ui_compare.py.
.EXAMPLE
  cargo build --release
  tools\ui_shots.ps1 -Out C:\temp\before     # before the change
  tools\ui_shots.ps1 -Out C:\temp\after      # after it
  python tools\ui_compare.py C:\temp\before C:\temp\after
#>
param([Parameter(Mandatory = $true)][string]$Out)
$exe = Join-Path $PSScriptRoot "..\target\release\dockview.exe"
New-Item -ItemType Directory -Force $Out | Out-Null
$looks = @(
  @{ name = "dark-en";  args = @("--theme","dark","--lang","en") },
  @{ name = "light-es"; args = @("--theme","light","--lang","es") }
)
$views = @(
  @{ name = "pill";     args = @() },
  @{ name = "settings"; args = @("--open-settings") },
  @{ name = "replay";   args = @("--open-replay") },
  @{ name = "sources";  args = @("--open-sources") },
  @{ name = "scenes";   args = @("--open-scenes") },
  @{ name = "stats";    args = @("--stats","--demo-findings") }
)
foreach ($look in $looks) {
  foreach ($view in $views) {
    $file = Join-Path $Out "$($look.name)-$($view.name).png"
    $argv = @("--test-pattern","--ui-preview") + $look.args + $view.args + @("--screenshot", $file)
    & $exe @argv 2>&1 | Out-Null
    if (-not (Test-Path $file)) { "MISSING $file" }
  }
}
"$((Get-ChildItem $Out -Filter *.png).Count) screenshots in $Out"
