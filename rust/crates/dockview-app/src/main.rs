//! Dockview: minimal low-latency viewer for UVC capture cards.
//!
//! Keys: `F`/`F11` fullscreen · `Esc` leave fullscreen · `V` next video device
//! · `A` next audio device · `↑`/`↓` volume · `←`/`→` audio sync (±10 ms)
//! · `P` render preset.

#[cfg(not(windows))]
compile_error!("Dockview only supports Windows.");

mod app;
mod auto_switch;
mod cli;
mod keys;
mod replay_service;
mod sources;
mod stats_log;
mod telemetry;

fn main() {
    app::run();
}
