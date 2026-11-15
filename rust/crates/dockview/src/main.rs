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

// Elsewhere than Windows the program only says it has no adapters yet, so the
// window's code (which does build there) is not used.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod capture_overlay;
mod os;
#[cfg(windows)]
mod run;
mod shell;
#[cfg(windows)]
mod snap;

#[cfg(windows)]
fn main() {
    run::run();
}

/// Dockview only has its system adapters for Windows so far (Linux is a goal of
/// 1.0); elsewhere it says so and leaves.
#[cfg(not(windows))]
fn main() {
    use std::io::Write;
    let _ = writeln!(
        std::io::stderr(),
        "Dockview does not have an adapter for this system yet."
    );
    std::process::exit(1);
}
