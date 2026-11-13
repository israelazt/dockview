//! Dockview core: capture, audio, stream selection and settings.
//! Platform-independent logic lives here so it can be unit tested.

#[cfg(windows)]
pub mod audio;
#[cfg(windows)]
pub mod capture;
pub mod compose;
#[cfg(windows)]
pub mod encode;
pub mod gpu;
#[cfg(windows)]
pub mod mf;
#[cfg(windows)]
pub mod recorder;
#[cfg(windows)]
pub mod recording;
#[cfg(windows)]
pub mod replay;
#[cfg(windows)]
pub mod screen;
pub mod settings;
#[cfg(windows)]
pub mod tap_capture;
#[cfg(windows)]
pub mod targets;
pub mod testsrc;
pub mod worker;

// The domain lives in its own crate. These re-exports keep the old paths
// (`dockview_core::scene`, `crate::frame`…) working until the crates are renamed
// and every user imports `dockview_domain` directly (see ARCHITECTURE.md).
pub use dockview_domain::{
    audio_sync, device, device_names, diagnostics, frame, geometry, pcm, scene, stream, transition,
};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
