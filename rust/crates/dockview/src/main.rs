//! Dockview: minimal low-latency viewer for UVC capture cards.
//!
//! Keys: `F`/`F11` fullscreen · `Esc` leave fullscreen · `V` next video device
//! · `A` next audio device · `↑`/`↓` volume · `←`/`→` audio sync (±10 ms)
//! · `P` render preset · `Tab` Studio panel.
//!
//! This crate is only the window and the wiring: it creates the real adapters,
//! hands them to the application (`dockview-app`) and draws what it asks to
//! show. The decisions are not here. See `rust/ARCHITECTURE.md`.

// The release build is a window program: no console appears on a double click
// (a terminal it was started from is attached to in `run`). Debug builds keep
// the console, to see the log while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Dockview only supports Windows.");

mod run;
mod shell;

fn main() {
    run::run();
}
