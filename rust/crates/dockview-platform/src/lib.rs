//! Dockview platform: the Windows side. Capture, audio, the encoder, the GPU,
//! the recorder and what is saved to disk. It implements the ports defined in
//! `dockview-domain`. See `rust/ARCHITECTURE.md`.

#[cfg(windows)]
pub mod audio;
#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod clips;
pub mod compose;
#[cfg(windows)]
pub mod encode;
pub mod gpu;
#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
pub mod mf;
#[cfg(windows)]
pub mod perf;
#[cfg(windows)]
pub mod recorder;
#[cfg(windows)]
pub mod recording;
#[cfg(windows)]
pub mod render;
#[cfg(windows)]
pub mod replay_machine;
#[cfg(windows)]
pub mod screen;
pub mod settings_store;
#[cfg(windows)]
pub mod source_machine;
#[cfg(windows)]
pub mod system_lang;
#[cfg(windows)]
pub mod tap_capture;
#[cfg(windows)]
pub mod targets;
pub mod testsrc;
#[cfg(windows)]
pub mod ui_gpu;
pub mod worker;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
