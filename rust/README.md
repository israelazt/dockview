# Dockview (Rust)

A minimal, low-latency viewer for UVC capture cards on Windows, with instant
replay, recording and scenes. This is the version under development, on the
branch `migration/rust`; the original C#/WPF app in the repository root is kept
only as a reference until the migration ends.

## Layout

Five crates under `crates/`, with dependencies going only downward (a test
guards it):

| Crate | What it is |
|---|---|
| `dockview-domain` | Rules and data without Windows, disk or window: scenes, settings, recording, diagnosis, texts, the view model and the ports (traits) |
| `dockview-platform` | The Windows side: capture, audio, H.264 encoder, GPU, compositor, recorder, hotkeys, settings on disk. Implements the ports |
| `dockview-ui` | The interface: it builds a frame of controls from a `Model` and returns `Action`s |
| `dockview-app` | `Core`: the services and what each action does, against ports only. Has a test rig (`src/testing/`) with fakes of every port |
| `dockview` | The program: the window, the event loop and the wiring of the real adapters |

Read `ARCHITECTURE.md` before adding anything: it has the rules and the recipe
for a new feature. The other documents are plans, each with its own status:
`HARDENING_PLAN.md` (robustness and quality, the one in progress),
`STUDIO_PLAN.md` (the side panel), `CAPTURE_PLAN.md`, `REPLAY_PLAN.md`,
`UI_PLAN.md`, `PLUGINS_PLAN.md`; `PLAN.md` is the historical migration plan.

## Build, test and run

Windows 10/11 x64 and a recent stable Rust toolchain.

```powershell
cd rust
cargo build --release -p dockview      # target\release\dockview.exe, one file
cargo run -p dockview                  # run it (debug build)
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

Some tests use the GPU and the H.264 encoder and take about a quarter of a
minute. To skip them (a CI runner, or a PC that is busy with a game):

```powershell
$env:DOCKVIEW_NO_HARDWARE = "1"; cargo test --workspace
```

## Command line

`dockview.exe [options]`: `--video NAME` (a card, by part of its name),
`--screen N` or `--window TEXT` (capture a monitor or a window instead),
`--pip KIND:VALUE` and `--pip-size N` (a second source in a corner),
`--record`, `--mic`, `--stats`, `--log-stats FILE.csv`, `--theme auto|dark|light`,
`--lang auto|en|es`, `--test-pattern` (colour bars, no card needed),
`--screenshot FILE.png` (draw a few frames, save and exit) and `--ui-preview`
(fake devices and fixed settings, for screenshots).

Keys: `F`/`F11` full screen, `Esc` leave it, `V` next card, `A` next audio
input, `Up`/`Down` volume, `Left`/`Right` audio sync, `P` render preset,
`I` statistics.

Settings are kept in `%APPDATA%\Dockview\settings.json` (a damaged file is kept
as `settings.corrupt.json`). The log is `%APPDATA%\Dockview\dockview.log` (it turns
over at 1 MB into `dockview.log.old`); the release build has no console window,
but from a terminal it prints there as well.

The fonts are cut down to what the interface draws: after adding an icon or a
new kind of text, run `python tools/subset_fonts.py` (needs `pip install
fonttools`; the full fonts are in `crates/dockview-ui/assets/fonts/full/`). Tests
fail if a text or an icon is missing from the cut fonts.

## Checking the interface

`tools\measure.ps1` takes the performance baseline (it opens the program with the
capture card for about 16 minutes; do not run it while playing) and
`tools\stats_summary.py` summarises a `--log-stats` file or the start-up times in
the log.

`tools\ui_shots.ps1` takes the same screenshots every time and `tools\ui_compare.py`
compares two sets pixel by pixel (it needs Python with Pillow and numpy).
