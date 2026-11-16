<#
.SYNOPSIS
  Takes the performance baseline of Dockview (block 7 of HARDENING_PLAN.md).

.DESCRIPTION
  Runs the release program through four situations and summarises each one:
  just viewing, recording, a scene with a second source in the corner, and a
  cold start. It OPENS THE PROGRAM WITH THE CAPTURE CARD and runs for several
  minutes: do not run it while playing on that PC.

  Needs a release build (cargo build --release -p dockview) and Python.

.PARAMETER Minutes
  How long each of the three long runs lasts (default 5).

.PARAMETER Video
  Part of the capture card's name (default UGREEN).

.PARAMETER PipMonitor
  The monitor shown in the corner for the scene run (default 1; 0 is the primary).

.EXAMPLE
  .\tools\measure.ps1 -Minutes 5
#>
param(
    [double]$Minutes = 5,
    [string]$Video = "UGREEN",
    [int]$PipMonitor = 1,
    [string]$Out = (Join-Path $PSScriptRoot "..\measurements\$(Get-Date -Format 'yyyy-MM-dd_HHmm')")
)

$ErrorActionPreference = "Stop"
$exe = Join-Path $PSScriptRoot "..\target\release\dockview.exe"
if (-not (Test-Path $exe)) { throw "Build it first: cargo build --release -p dockview" }
$summary = Join-Path $PSScriptRoot "stats_summary.py"
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$log = Join-Path $env:APPDATA "Dockview\dockview.log"

function Invoke-Run([string]$Name, [string[]]$Arguments, [double]$Seconds) {
    Write-Host "== $Name ($Seconds s)" -ForegroundColor Cyan
    $p = Start-Process -FilePath $exe -ArgumentList $Arguments -PassThru
    Start-Sleep -Seconds $Seconds
    if (-not $p.HasExited) {
        [void]$p.CloseMainWindow()
        if (-not $p.WaitForExit(15000)) { $p.Kill() }
    }
}

$long = $Minutes * 60
$runs = @(
    @{ Name = "view";   Args = @("--video", $Video) },
    @{ Name = "record"; Args = @("--video", $Video, "--record") },
    @{ Name = "scene";  Args = @("--video", $Video, "--pip", "monitor:$PipMonitor") }
)
foreach ($run in $runs) {
    $csv = Join-Path $Out "$($run.Name).csv"
    Invoke-Run $run.Name ($run.Args + @("--log-stats", $csv, "--stats")) $long
    python $summary $csv | Tee-Object -FilePath (Join-Path $Out "$($run.Name).txt")
}

# Cold start: a run long enough to show the first picture; the log has the times.
Invoke-Run "cold start" @("--video", $Video) 20
python $summary --startup $log | Tee-Object -FilePath (Join-Path $Out "startup.txt")

Write-Host "`nSaved in $Out" -ForegroundColor Green
