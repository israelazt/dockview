//! Dockview core: capture, audio, stream selection and settings.
//! Platform-independent logic lives here so it can be unit tested.

pub mod device;
pub mod settings;
pub mod stream;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
