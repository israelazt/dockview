//! Dockview core: capture, audio, stream selection and settings.
//! Platform-independent logic lives here so it can be unit tested.

#[cfg(windows)]
pub mod audio;
pub mod audio_sync;
#[cfg(windows)]
pub mod capture;
pub mod compose;
pub mod device;
pub mod device_names;
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
pub mod recording;
#[cfg(windows)]
pub mod replay;
pub mod scene;
#[cfg(windows)]
pub mod screen;
pub mod settings;
pub mod stream;
#[cfg(windows)]
pub mod tap_capture;
pub mod testsrc;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
