# Fexarium

A family of Windows apps for people who play, stream and record:

- **Luminifex** — live viewer and Studio mode (capture, scenes, recording, replay, streaming).
- **Capifex** — screenshots (a replacement for the Snipping Tool). Being split out of Luminifex.
- **Motifex** — video editor. Reserved; not started.

This repository is a Rust workspace (a monorepo). The plan to split the app into the family is
[`docs/FEXARIUM_PLAN.md`](docs/FEXARIUM_PLAN.md). The old C#/WPF viewer was removed from this
branch; it is still in `main`. Until the split is done, the code still uses the old name
(`dockview-*` crates, `dockview.exe`).

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
for a new feature. What the product is and what it aims for (under its future name,
Luminifex) is in `docs/LUMINIFEX.md`. The other documents are plans, each with its own
status:
`docs/ROADMAP.md` (what is left, in order, with the decisions that block it),
`docs/STREAMER_PLAN.md` (what a gaming streamer needs, what we have and what is missing),
`docs/HARDENING_PLAN.md` (robustness and quality),
`docs/STUDIO_PLAN.md` (the side panel), `docs/CAPTURE_PLAN.md`, `docs/REPLAY_PLAN.md`,
`docs/UI_PLAN.md`, `docs/PLUGINS_PLAN.md`; `docs/PLAN.md` is the historical migration plan.

## Build, test and run

Windows 10/11 x64 and a recent stable Rust toolchain.

```powershell
cargo build --release -p dockview      # target\release\dockview.exe, one file
cargo run -p dockview                  # run it (debug build)
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

### Installing, and developing as before

Developing does not change: `cargo run`, `target/release/dockview.exe` and the
tools work exactly as above, with or without the installer. A development run
and an installed copy share the same settings (`%APPDATA%\Dockview`).

To make what a user gets, `tools/build_installer.ps1` builds the release and
packages it into `dist/`: an installer (`Dockview-Setup-<version>.exe`,
per user by default, no administrator, Start menu entry, uninstaller that asks
whether to delete the settings) and a portable zip with just `Dockview.exe`,
each with a `.sha256`. It needs [Inno Setup 6](https://jrsoftware.org/isdl.php)
(`winget install JRSoftware.InnoSetup`). `tools/test_installer.ps1` installs it
into a scratch folder, checks it and uninstalls; the CI does both whenever the
installer changes (`installer.yml`) and on each release tag (`release.yml`).
The version is the one in `Cargo.toml`; `dockview.exe --version` prints it.
An unsigned installer makes SmartScreen warn: `build_installer.ps1 -Sign` signs it
when a certificate is given (`DOCKVIEW_SIGN_PFX`, `DOCKVIEW_SIGN_PASSWORD`).

Some tests use the GPU and the H.264 encoder and take about a quarter of a
minute. To skip them (a CI runner, or a PC that is busy with a game):

```powershell
$env:DOCKVIEW_NO_HARDWARE = "1"; cargo test --workspace
```

## Going live

Studio mode, **Stream** section: choose the service (Twitch, YouTube, Kick or another), paste the stream key (`Ctrl+V`, then Enter; it is kept encrypted for your Windows user and never shown again), pick the frame rate and the bitrate and press **Go live**. What the viewer shows goes out, with the sound of the game and the microphone. The tab row shows *Connecting…* and then *Live*; if the connection drops it reconnects by itself.

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
