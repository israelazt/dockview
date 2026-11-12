//! Dockview core: capture, audio, stream selection and settings.
//! Platform-independent logic lives here so it can be unit tested.

#[cfg(windows)]
pub mod audio;
pub mod audio_sync;
#[cfg(windows)]
pub mod capture;
pub mod device;
pub mod diagnostics;
#[cfg(windows)]
pub mod encode;
pub mod frame;
#[cfg(windows)]
pub mod mf;
pub mod pcm;
#[cfg(windows)]
pub mod recorder;
#[cfg(windows)]
pub mod replay;
pub mod settings;
pub mod stream;
pub mod testsrc;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
